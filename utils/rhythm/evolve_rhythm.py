#!/usr/bin/env python3
"""Evolve a YM2608 ADPCM-A rhythm ROM using hybrid evolution + gradient descent.

Phase 1 - Phase-domain optimization:
  Decompose the target PCM into magnitude and phase via FFT. Fix the magnitude
  spectrum and optimize the phase using a hybrid approach:
    a) Evolutionary search for global exploration (short burst)
    b) Gradient descent (Adam via JAX autodiff) for precise local refinement

  Multi-resolution mel-spectrogram fitness with temporal weighting ensures
  both spectral accuracy and clean temporal envelopes. Minimum-phase
  initialization provides a strong starting point for tonal instruments.

Phase 2 - ADPCM encoding:
  Lookahead ADPCM encoder targeting the optimized PCM.

Usage:
    python3 evolve_rhythm.py <input_rom> <output_rom> [--plot]

Dependencies: numpy, numba, jax
"""

import argparse
import os
import sys
import time

os.environ.setdefault("TF_CPP_MIN_LOG_LEVEL", "3")

import numba
import numpy as np
from numba import njit

import jax
import jax.numpy as jnp
try:
    _t = jnp.array(np.ones(4, dtype=np.float64))
    _t.block_until_ready()
except Exception:
    jax.config.update("jax_platform_name", "cpu")
    import jax.numpy as jnp

ADPCM_A_STEPS = np.array([
    16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253,
    279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876,
    963, 1060, 1166, 1282, 1411, 1552,
], dtype=np.int32)

ADPCM_A_STEP_INC = np.array([-1, -1, -1, -1, 2, 5, 7, 9], dtype=np.int32)

INSTRUMENTS = [
    ("BD",  0x0000, 0x01BF),
    ("SD",  0x01C0, 0x043F),
    ("TC",  0x0440, 0x1B7F),
    ("HH",  0x1B80, 0x1CFF),
    ("TOM", 0x1D00, 0x1F7F),
    ("RIM", 0x1F80, 0x1FFF),
]

ROM_SIZE = 8192

INSTRUMENT_CONFIG = {
    "BD": {
        "multi_res": [128, 256],
        "grad_steps": 30000,
        "lr": 0.003,
        "env_weight": 0.3,
        "target": 0.5,
        "evo_stall": 2000,
        "skip_evo": False,
    },
    "SD": {
        "multi_res": None,
        "grad_steps": 5000,
        "lr": 0.01,
        "env_weight": 0.3,
        "target": 0.5,
        "evo_stall": 50000,
        "skip_evo": False,
    },
    "TC": {
        "multi_res": [128, 256, 512],
        "grad_steps": 30000,
        "lr": 0.003,
        "env_weight": 0.3,
        "target": 0.5,
        "evo_stall": 2000,
        "skip_evo": True,
    },
    "HH": {
        "multi_res": None,
        "grad_steps": 5000,
        "lr": 0.01,
        "env_weight": 0.3,
        "target": 0.5,
        "evo_stall": 50000,
        "skip_evo": False,
    },
    "TOM": {
        "multi_res": [128, 256, 512],
        "grad_steps": 500000,
        "lr": 0.005,
        "env_weight": 0.5,
        "target": 0.5,
        "evo_stall": 2000,
        "skip_evo": True,
    },
    "RIM": {
        "multi_res": None,
        "grad_steps": 5000,
        "lr": 0.01,
        "env_weight": 0.3,
        "target": 0.4,
        "evo_stall": 50000,
        "skip_evo": False,
    },
}

DEFAULT_CONFIG = {
    "multi_res": None,
    "grad_steps": 5000,
    "lr": 0.01,
    "env_weight": 0.3,
    "target": 1.25,
    "evo_stall": 50000,
    "skip_evo": False,
}

ADPCM_LOOKAHEAD = 4

POPULATION_SIZE = 2048
TOURNAMENT_SIZE = 5
ELITE_COUNT = 6
N_CHILDREN = POPULATION_SIZE - ELITE_COUNT
GENERATION_LIMIT = 15_000_000

NOMINAL_SAMPLE_RATE = 110933

KNOWN_SEEDS = {
    "BD":  [831309432],
    "SD":  [203541028],
    "TC":  [76030938],
    "HH":  [954193737],
    "TOM": [123123167],
    "RIM": [1308206982],
}


@njit(cache=True)
def adpcm_step(acc, step_idx, nibble, steps, step_inc):
    nib = nibble & 0xF
    mag = nib & 0x7
    sign = nib & 0x8
    delta = (2 * mag + 1) * steps[step_idx] // 8
    if sign:
        delta = -delta
    acc = (acc + delta) & 0xFFF
    step_idx = min(48, max(0, step_idx + step_inc[mag]))
    raw16 = (acc << 4) & 0xFFFF
    if raw16 >= 0x8000:
        raw16 -= 0x10000
    pcm = ((raw16 * 15) >> 5) & ~3
    return acc, step_idx, numba.int16(pcm)


@njit(cache=True)
def decode_adpcm_a(nibbles, steps, step_inc):
    n = len(nibbles)
    pcm = np.empty(n, dtype=np.int16)
    acc, si = 0, 0
    for i in range(n):
        acc, si, pcm[i] = adpcm_step(acc, si, nibbles[i], steps, step_inc)
    return pcm


@njit(cache=True)
def encode_adpcm_a(target_pcm, steps, step_inc, lookahead):
    n = len(target_pcm)
    nibbles = np.empty(n, dtype=np.uint8)
    acc, si = 0, 0
    for i in range(n):
        target = numba.int32(target_pcm[i])
        best_nib = numba.uint8(0)
        best_total_err = numba.int64(0x7FFFFFFFFFFFFFFF)
        best_acc, best_si = acc, si
        for nib in range(16):
            ta, ts, tp = adpcm_step(acc, si, nib, steps, step_inc)
            total_err = numba.int64(abs(numba.int32(tp) - target))
            la_acc, la_si = ta, ts
            ahead = min(lookahead, n - i - 1)
            for j in range(1, ahead + 1):
                la_target = numba.int32(target_pcm[i + j])
                la_best_err = numba.int32(0x7FFFFFFF)
                la_best_acc, la_best_si = la_acc, la_si
                for la_nib in range(16):
                    la_ta, la_ts, la_tp = adpcm_step(
                        la_acc, la_si, la_nib, steps, step_inc)
                    la_err = abs(numba.int32(la_tp) - la_target)
                    if la_err < la_best_err:
                        la_best_err = la_err
                        la_best_acc = la_ta
                        la_best_si = la_ts
                total_err += numba.int64(la_best_err)
                la_acc, la_si = la_best_acc, la_best_si
            if total_err < best_total_err:
                best_total_err = total_err
                best_nib = numba.uint8(nib)
                best_acc = ta
                best_si = ts
        nibbles[i] = best_nib
        acc, si = best_acc, best_si
    return nibbles


@njit(cache=True)
def bytes_to_nibbles(data):
    n = len(data)
    nibbles = np.empty(n * 2, dtype=np.uint8)
    for i in range(n):
        nibbles[i * 2] = (data[i] >> 4) & 0xF
        nibbles[i * 2 + 1] = data[i] & 0xF
    return nibbles


@njit(cache=True)
def nibbles_to_bytes(nibbles):
    n = len(nibbles) // 2
    data = np.empty(n, dtype=np.uint8)
    for i in range(n):
        data[i] = ((nibbles[i * 2] & 0xF) << 4) | (nibbles[i * 2 + 1] & 0xF)
    return data


@njit(cache=True)
def byte_distance(nibbles_a, nibbles_b):
    n = len(nibbles_a) // 2
    count = 0
    for i in range(n):
        ba = ((nibbles_a[i * 2] & 0xF) << 4) | (nibbles_a[i * 2 + 1] & 0xF)
        bb = ((nibbles_b[i * 2] & 0xF) << 4) | (nibbles_b[i * 2 + 1] & 0xF)
        if ba != bb:
            count += 1
    return count


@jax.jit
def _evolution_step(population, fitness_values, key, strength, mutation_rate):
    key, k1, k2, k3, k4, k5, k6, k7 = jax.random.split(key, 8)
    n_bins = population.shape[1]

    sorted_indices = jnp.argsort(fitness_values)
    elites = population[sorted_indices[:ELITE_COUNT]]

    tidx_a = jax.random.randint(k1, (N_CHILDREN, TOURNAMENT_SIZE), 0, POPULATION_SIZE)
    winners_a = tidx_a[jnp.arange(N_CHILDREN), jnp.argmin(fitness_values[tidx_a], axis=1)]
    parents_a = population[winners_a]

    tidx_b = jax.random.randint(k2, (N_CHILDREN, TOURNAMENT_SIZE), 0, POPULATION_SIZE)
    winners_b = tidx_b[jnp.arange(N_CHILDREN), jnp.argmin(fitness_values[tidx_b], axis=1)]
    parents_b = population[winners_b]

    alpha = jax.random.uniform(k3, parents_a.shape)
    diff = (parents_b - parents_a + jnp.pi) % (2 * jnp.pi) - jnp.pi
    blend = parents_a + alpha * diff

    points = jnp.sort(jax.random.randint(k4, (N_CHILDREN, 2), 0, n_bins), axis=1)
    cols = jnp.arange(n_bins)[jnp.newaxis, :]
    tp_mask = (cols >= points[:, 0:1]) & (cols < points[:, 1:2])
    twopoint = jnp.where(tp_mask, parents_b, parents_a)

    use_blend = jax.random.uniform(k5, (N_CHILDREN, 1)) < 0.5
    children = jnp.where(use_blend, blend, twopoint)

    mut_mask = jax.random.uniform(k6, children.shape) < mutation_rate
    deltas = jax.random.uniform(k7, children.shape, minval=-strength, maxval=strength)
    children = children + deltas * mut_mask

    return jnp.concatenate([elites, children], axis=0), key


def _next_pow2(x):
    p = 1
    while p < x:
        p *= 2
    return p


def compute_minimum_phase(magnitude, n_samples):
    """Minimum-phase reconstruction via cepstral method.

    For a given magnitude spectrum, returns the phase that concentrates
    energy at the beginning of the signal -- ideal for percussive sounds.
    """
    if n_samples % 2 == 0:
        full_mag = np.concatenate([magnitude, magnitude[-2:0:-1]])
    else:
        full_mag = np.concatenate([magnitude, magnitude[-1:0:-1]])
    log_mag = np.log(np.maximum(full_mag, 1e-10))
    cepstrum = np.fft.ifft(log_mag).real
    n_cep = len(cepstrum)
    window = np.zeros(n_cep)
    window[0] = 1.0
    window[1:(n_cep + 1) // 2] = 2.0
    if n_cep % 2 == 0:
        window[n_cep // 2] = 1.0
    analytic = np.fft.fft(cepstrum * window)
    return np.imag(analytic[:len(magnitude)])


def _make_mel_filterbank(sample_rate, n_fft, n_mels):
    num_bins = n_fft // 2 + 1
    low_mel = 2595.0 * np.log10(1.0 + 0.0 / 700.0)
    high_mel = 2595.0 * np.log10(1.0 + (sample_rate / 2) / 700.0)
    mel_pts = np.linspace(low_mel, high_mel, n_mels + 2)
    hz_pts = 700.0 * (10.0 ** (mel_pts / 2595.0) - 1.0)
    bins = np.floor((n_fft + 1) * hz_pts / sample_rate).astype(int)

    fb = np.zeros((n_mels, num_bins), dtype=np.float32)
    for m in range(n_mels):
        left, center, right = bins[m], bins[m + 1], bins[m + 2]
        for k in range(left, center):
            if center > left:
                fb[m, k] = (k - left) / (center - left)
        for k in range(center, right):
            if right > center:
                fb[m, k] = (right - k) / (right - center)
    return fb


class Fitness:
    """JAX-accelerated perceptual fitness with multi-resolution STFT.

    Builds JIT-compiled functions for both batch evaluation (evolution) and
    single-sample value_and_grad (gradient descent via Adam).
    """

    def __init__(self, target_pcm, target_mag, sample_rate, n_mels=48,
                 multi_resolution=None, envelope_weight=0.3):
        n = len(target_pcm)

        if multi_resolution is None:
            resolutions = [min(512, max(32, _next_pow2(n // 4)))]
        else:
            resolutions = [r for r in multi_resolution if r <= n]
            if not resolutions:
                resolutions = [min(512, max(32, _next_pow2(n // 4)))]

        res_fbs = []
        res_wins = []
        res_idxs = []
        res_target_mels = []
        res_temporal_weights = []
        res_nffts = []

        for n_fft in resolutions:
            hop_length = max(1, n_fft // 4)
            mel_fb = _make_mel_filterbank(sample_rate, n_fft, n_mels)
            n_frames = 1 + (n - n_fft) // hop_length
            if n_frames < 1:
                continue
            frame_starts = np.arange(n_frames) * hop_length
            frame_idx = frame_starts[:, np.newaxis] + np.arange(n_fft)
            window = np.hanning(n_fft).astype(np.float32)

            fb_j = jnp.array(mel_fb)
            win_j = jnp.array(window)
            idx_j = jnp.array(frame_idx)

            target_f32 = jnp.array(target_pcm.astype(np.float32))[None, :]
            frames = target_f32[:, idx_j] * win_j
            spec = jnp.fft.rfft(frames, n=n_fft, axis=-1)
            mel = jnp.matmul(fb_j, jnp.transpose(jnp.abs(spec), (0, 2, 1)))
            target_mel_j = jnp.log(mel + 1e-10)

            frame_energy = jnp.mean(jnp.abs(target_mel_j[0]), axis=0)
            temporal_weight = jnp.sqrt(
                frame_energy / (jnp.max(frame_energy) + 1e-10))
            temporal_weight = temporal_weight / (
                jnp.mean(temporal_weight) + 1e-10)

            res_fbs.append(fb_j)
            res_wins.append(win_j)
            res_idxs.append(idx_j)
            res_target_mels.append(target_mel_j)
            res_temporal_weights.append(temporal_weight)
            res_nffts.append(n_fft)

        env_hop = max(1, min(32, n // 4))
        env_trim = (n // env_hop) * env_hop

        target_f32_full = jnp.array(target_pcm.astype(np.float32))[None, :]
        env_frames = target_f32_full[:, :env_trim].reshape(1, -1, env_hop)
        target_env_j = jnp.sqrt(jnp.mean(env_frames ** 2, axis=-1))

        mag_j = jnp.array(target_mag.astype(np.float32))
        env_w = float(envelope_weight)
        n_res = len(res_fbs)

        res_data = tuple(zip(res_fbs, res_wins, res_idxs, res_target_mels,
                             res_temporal_weights, res_nffts))

        def _compute_batch(phases):
            phases_f32 = phases.astype(jnp.float32)
            fft_data = mag_j * jnp.exp(1j * phases_f32)
            pcm = jnp.fft.irfft(fft_data, n=n, axis=-1)
            pcm = jnp.clip(pcm, -32768, 32767)

            mel_dist = jnp.zeros(pcm.shape[0])
            for fb, win, idx, tgt_mel, tw, nf in res_data:
                fr = pcm[:, idx] * win
                sp = jnp.fft.rfft(fr, n=nf, axis=-1)
                ml = jnp.matmul(fb, jnp.transpose(jnp.abs(sp), (0, 2, 1)))
                log_mel = jnp.log(ml + 1e-10)
                diff_sq = (log_mel - tgt_mel) ** 2
                weighted = diff_sq * tw[None, None, :]
                mel_dist = mel_dist + jnp.mean(weighted, axis=(1, 2))
            mel_dist = mel_dist / n_res

            ef = pcm[:, :env_trim].reshape(pcm.shape[0], -1, env_hop)
            env = jnp.sqrt(jnp.mean(ef ** 2, axis=-1))
            env_dist = jnp.mean((env - target_env_j) ** 2, axis=1)

            return mel_dist + env_w * env_dist

        def _compute_single(phases):
            return _compute_batch(phases[None, :])[0]

        self._evaluate = jax.jit(_compute_batch)
        self._loss_and_grad = jax.jit(jax.value_and_grad(_compute_single))

    def __call__(self, population):
        return self._evaluate(population)


def phase_to_pcm(magnitude, phase, n_samples):
    fft = magnitude * np.exp(1j * phase)
    pcm = np.fft.irfft(fft, n=n_samples)
    return np.clip(pcm, -32768, 32767).astype(np.int16)


def gradient_refine(phases, fitness_obj, n_steps, lr=0.01, verbose=True):
    """Refine phase vector using Adam with cosine LR annealing.
    Returns (best_phases_numpy, best_loss)."""
    m = jnp.zeros_like(phases)
    v = jnp.zeros_like(phases)
    best_phases = phases
    best_loss = float('inf')

    t0 = time.time()
    for step in range(1, n_steps + 1):
        loss, grads = fitness_obj._loss_and_grad(phases)

        cosine_decay = 0.5 * (1.0 + np.cos(np.pi * step / n_steps))
        current_lr = lr * cosine_decay
        lr_j = jnp.array(current_lr, dtype=phases.dtype)

        t = jnp.array(float(step), dtype=phases.dtype)
        m = 0.9 * m + 0.1 * grads
        v = 0.999 * v + 0.001 * grads ** 2
        m_hat = m / (1.0 - 0.9 ** t)
        v_hat = v / (1.0 - 0.999 ** t)
        phases = phases - lr_j * m_hat / (jnp.sqrt(v_hat) + 1e-8)

        loss_val = float(loss)
        if loss_val < best_loss:
            best_loss = loss_val
            best_phases = phases

        if verbose and step % 1000 == 0:
            elapsed = time.time() - t0
            print(f"      Adam step {step}: loss={best_loss:.4f}  "
                  f"lr={current_lr:.5f}  [{elapsed:.1f}s]")

    return np.asarray(best_phases), best_loss


def evolve_pcm(name, target_pcm, target_mag, fitness, key, seed,
               config, verbose=True):
    """Optimize phase spectrum using hybrid evolution + gradient descent.

    The magnitude spectrum is locked to the target's (preserving timbre).
    Minimum-phase initialization provides a strong starting point.
    For tonal instruments, gradient descent via JAX autodiff dramatically
    improves convergence over pure evolution."""
    n = len(target_pcm)
    target_phase = np.angle(np.fft.rfft(target_pcm.astype(np.float64)))
    n_bins = len(target_mag)
    fitness_target = config["target"]

    if verbose:
        mode = "gradient-only" if config["skip_evo"] else "hybrid evo+gradient"
        print(f"  Phase 1 [seed={seed}]: {mode} ({n} samples, "
              f"{n_bins} bins, target < {fitness_target})...",
              flush=True)

    min_phase = compute_minimum_phase(target_mag, n)

    if config["skip_evo"]:
        if verbose:
            print(f"    Gradient descent from minimum-phase init "
                  f"({config['grad_steps']} steps, lr={config['lr']})...")
        best_phase_j = jnp.array(min_phase)
        best_phase, best_fitness = gradient_refine(
            best_phase_j, fitness, config["grad_steps"], config["lr"], verbose)
        stalled = False
    else:
        key, init_key1, init_key2 = jax.random.split(key, 3)
        half = POPULATION_SIZE // 2
        noise = jax.random.normal(init_key1, (half, n_bins)) * 0.5
        pop_minphase = jnp.array(min_phase)[None, :] + noise
        pop_random = jax.random.uniform(
            init_key2, (POPULATION_SIZE - half, n_bins),
            minval=-jnp.pi, maxval=jnp.pi)
        population = jnp.concatenate([pop_minphase, pop_random], axis=0)
        population = population.at[0].set(jnp.array(min_phase))
        population = population.at[:, 0].set(target_phase[0])

        fitness_values = fitness(population)
        best_idx = int(jnp.argmin(fitness_values))
        best_fitness = float(fitness_values[best_idx])
        best_phase = np.asarray(population[best_idx])

        if verbose:
            best_pcm = phase_to_pcm(target_mag, best_phase, n)
            pcm_diff = np.mean(np.abs(target_pcm.astype(np.float64) -
                                      best_pcm.astype(np.float64)))
            print(f"    Initial: fitness={best_fitness:.4f}  "
                  f"mean_sample_diff={pcm_diff:.0f}")

        stall_limit = config["evo_stall"]
        t0 = time.time()
        gen = 0
        stall_count = 0
        while best_fitness >= fitness_target:
            gen += 1

            if gen >= GENERATION_LIMIT:
                if verbose:
                    elapsed = time.time() - t0
                    print(f"    Hard limit at gen {gen}  [{elapsed:.1f}s]")
                break

            decay = 1.0 / (1.0 + 0.0003 * gen)
            strength = 0.02 + (np.pi - 0.02) * decay
            mutation_rate = 0.03 + (0.20 - 0.03) * decay

            population, key = _evolution_step(population, fitness_values, key,
                                              strength, mutation_rate)
            fitness_values = fitness(population)

            gen_best_idx = int(jnp.argmin(fitness_values))
            gen_best = float(fitness_values[gen_best_idx])
            if gen_best < best_fitness:
                best_fitness = gen_best
                best_phase = np.asarray(population[gen_best_idx])
                stall_count = 0
            else:
                stall_count += 1

            if stall_count >= stall_limit:
                if verbose:
                    elapsed = time.time() - t0
                    print(f"    Evolution stalled at gen {gen} [{elapsed:.1f}s]"
                          f", switching to gradient descent...")
                break

            if verbose and gen % 5000 == 0:
                elapsed = time.time() - t0
                best_pcm = phase_to_pcm(target_mag, best_phase, n)
                pcm_diff = np.mean(np.abs(target_pcm.astype(np.float64) -
                                          best_pcm.astype(np.float64)))
                corr = float(np.corrcoef(target_pcm.astype(np.float64),
                                         best_pcm.astype(np.float64))[0, 1])
                print(f"    Gen {gen:5d}: fitness={best_fitness:.4f}  "
                      f"mean_diff={pcm_diff:.0f}  corr={corr:.6f}  "
                      f"[{elapsed:.1f}s]")

        if best_fitness >= fitness_target and config["grad_steps"] > 0:
            grad_steps = config["grad_steps"]
            grad_lr = config["lr"]

            if verbose:
                print(f"    Gradient refinement from evo best (Adam, "
                      f"{grad_steps} steps, lr={grad_lr})...")
            evo_phase_j = jnp.array(best_phase)
            evo_refined, evo_fit = gradient_refine(
                evo_phase_j, fitness, grad_steps, grad_lr, verbose)

            if verbose:
                print(f"    Gradient refinement from min-phase (Adam, "
                      f"{grad_steps} steps, lr={grad_lr})...")
            mp_phase_j = jnp.array(min_phase)
            mp_refined, mp_fit = gradient_refine(
                mp_phase_j, fitness, grad_steps, grad_lr, verbose)

            if mp_fit < evo_fit:
                if verbose:
                    print(f"    Min-phase path won: {mp_fit:.4f} vs "
                          f"evo path {evo_fit:.4f}")
                if mp_fit < best_fitness:
                    best_fitness = mp_fit
                    best_phase = mp_refined
            else:
                if verbose:
                    print(f"    Evo path won: {evo_fit:.4f} vs "
                          f"min-phase path {mp_fit:.4f}")
                if evo_fit < best_fitness:
                    best_fitness = evo_fit
                    best_phase = evo_refined

        stalled = best_fitness >= fitness_target

    best_pcm = phase_to_pcm(target_mag, best_phase, n)

    if verbose:
        pcm_diff = np.mean(np.abs(target_pcm.astype(np.float64) -
                                   best_pcm.astype(np.float64)))
        corr = float(np.corrcoef(target_pcm.astype(np.float64),
                                  best_pcm.astype(np.float64))[0, 1])
        print(f"    Done [seed={seed}]: fitness={best_fitness:.4f}  "
              f"mean_diff={pcm_diff:.0f}  corr={corr:.6f}")

    return best_pcm, best_fitness, stalled


def evolve_instrument(name, original_nibbles, original_bytes, target_pcm,
                      seed_rng, verbose=True):
    steps = ADPCM_A_STEPS
    sinc = ADPCM_A_STEP_INC
    n_bytes = len(original_bytes)
    config = INSTRUMENT_CONFIG.get(name, DEFAULT_CONFIG)

    if verbose:
        print(f"\n{'='*60}")
        print(f"Evolving {name}: {len(original_nibbles)} nibbles ({n_bytes} "
              f"bytes)")
        print(f"{'='*60}")

    fitness_target = config["target"]
    multi_res = config["multi_res"]
    env_weight = config["env_weight"]

    target_fft = np.fft.rfft(target_pcm.astype(np.float64))
    target_mag = np.abs(target_fft)
    fitness = Fitness(target_pcm, target_mag, NOMINAL_SAMPLE_RATE,
                      multi_resolution=multi_res, envelope_weight=env_weight)

    best_pcm = None
    best_fitness = float('inf')
    best_seed = 0
    attempt = 0

    known = list(KNOWN_SEEDS.get(name, []))

    while best_fitness >= fitness_target:
        attempt += 1

        if known:
            seed = known.pop(0)
            if verbose:
                print(f"  Trying known seed {seed}...")
        else:
            seed = int(seed_rng.integers(0, 2**31))
            if verbose and attempt > 1:
                print(f"  Retry #{attempt} with seed {seed} "
                      f"(best so far: {best_fitness:.4f})...")

        key = jax.random.PRNGKey(seed)
        pcm, fit, stalled = evolve_pcm(name, target_pcm, target_mag, fitness,
                                       key, seed, config, verbose)
        if fit < best_fitness:
            best_fitness = fit
            best_pcm = pcm
            best_seed = seed

        if not stalled:
            break

    if verbose:
        print(f"  Phase 2: lookahead-{ADPCM_LOOKAHEAD} encoding evolved PCM "
              f"to ADPCM...", end=" ", flush=True)

    evolved_nibbles = encode_adpcm_a(best_pcm, steps, sinc, ADPCM_LOOKAHEAD)

    roundtrip_pcm = decode_adpcm_a(evolved_nibbles, steps, sinc)
    dist = byte_distance(evolved_nibbles, original_nibbles)
    pct = 100 * dist / n_bytes

    o = target_pcm.astype(np.float64)
    r = roundtrip_pcm.astype(np.float64)
    corr = float(np.corrcoef(o, r)[0, 1]) if np.std(o) > 0 else 1.0
    rms = float(np.sqrt(np.mean((o - r) ** 2)))
    psnr = 20 * np.log10(float(np.max(np.abs(o))) / rms) if rms > 0 else 999

    if verbose:
        print(f"done")
        print(f"  Result: {dist}/{n_bytes} bytes differ ({pct:.0f}%)  "
              f"corr={corr:.6f}  PSNR={psnr:.1f}dB")

    return evolved_nibbles, dist, corr, psnr, best_seed, best_fitness, attempt


def main():
    parser = argparse.ArgumentParser(
        description="Evolve a YM2608 rhythm ROM. Hybrid evolution + gradient "
                    "descent with multi-resolution perceptual fitness.")
    parser.add_argument("input_rom", help="Path to original 8KB ROM file")
    parser.add_argument("output_rom", help="Path to write evolved ROM")
    parser.add_argument("--plot", action="store_true",
                        help="Show comparison plots (needs matplotlib)")
    args = parser.parse_args()

    seed_rng = np.random.default_rng()

    with open(args.input_rom, "rb") as f:
        rom_data = np.frombuffer(f.read(), dtype=np.uint8)
    if len(rom_data) != ROM_SIZE:
        print(f"Error: ROM must be {ROM_SIZE} bytes, got {len(rom_data)}",
              file=sys.stderr)
        sys.exit(1)

    print(f"Input ROM:    {args.input_rom} ({ROM_SIZE} bytes)")
    print(f"Output ROM:   {args.output_rom}")
    print(f"Population:   {POPULATION_SIZE}")
    targets = ", ".join(
        f"{n}:<{INSTRUMENT_CONFIG.get(n, DEFAULT_CONFIG)['target']}"
        for n, _, _ in INSTRUMENTS)
    print(f"Targets:      {targets}")
    print(f"JAX backend:  {jax.default_backend()}")
    print(f"Method:       Phase 1 = hybrid evo + Adam gradient descent")
    print(f"              Phase 2 = lookahead-{ADPCM_LOOKAHEAD} ADPCM encoding")

    print("JIT compiling...", end=" ", flush=True)
    _w = np.zeros(4, dtype=np.uint8)
    decode_adpcm_a(_w, ADPCM_A_STEPS, ADPCM_A_STEP_INC)
    encode_adpcm_a(np.zeros(4, dtype=np.int16), ADPCM_A_STEPS,
                   ADPCM_A_STEP_INC, ADPCM_LOOKAHEAD)
    bytes_to_nibbles(np.zeros(2, dtype=np.uint8))
    nibbles_to_bytes(_w)
    byte_distance(_w, _w)
    print("done")

    if os.path.exists(args.output_rom):
        with open(args.output_rom, "rb") as f:
            existing = f.read(ROM_SIZE)
        existing = existing.ljust(ROM_SIZE, b"\x00")[:ROM_SIZE]
        with open(args.output_rom, "wb") as f:
            f.write(existing)
    else:
        with open(args.output_rom, "wb") as f:
            f.write(b"\x00" * ROM_SIZE)

    plot_data = []
    stats = []

    for name, start, end in INSTRUMENTS:
        byte_count = end - start + 1
        original_bytes = rom_data[start:start + byte_count]
        original_nibbles = bytes_to_nibbles(original_bytes)
        target_pcm = decode_adpcm_a(original_nibbles, ADPCM_A_STEPS,
                                    ADPCM_A_STEP_INC)

        result = evolve_instrument(
            name, original_nibbles, original_bytes, target_pcm, seed_rng)
        evolved_nibbles, dist, corr, psnr, seed, fitness_val, attempts = result

        stats.append((name, seed, fitness_val, corr, psnr, dist,
                       byte_count, attempts))

        evolved_bytes = nibbles_to_bytes(evolved_nibbles)
        with open(args.output_rom, "r+b") as f:
            f.seek(start)
            f.write(evolved_bytes.tobytes())
        print(f"  Wrote {name} to {args.output_rom} @ 0x{start:04X}")

        if args.plot:
            evolved_pcm = decode_adpcm_a(evolved_nibbles, ADPCM_A_STEPS,
                                         ADPCM_A_STEP_INC)
            plot_data.append((name, target_pcm, evolved_pcm))

    total_diff = sum(dist for _, _, _, _, _, dist, _, _ in stats)
    total_bytes = sum(n_bytes for _, _, _, _, _, _, n_bytes, _ in stats)

    print(f"\n{'='*80}")
    print(f"{'Sample':<6} {'Seed':>12} {'Fitness':>10} {'Corr':>10} "
          f"{'PSNR':>8}  {'Diff':>12} {'Attempts':>9}")
    print(f"{'-'*80}")
    for name, seed, fitness_val, corr, psnr, dist, n_bytes, attempts in stats:
        print(f"{name:<6} {seed:>12} {fitness_val:>10.4f} {corr:>10.6f} "
              f"{psnr:>7.1f}dB  {dist:>5}/{n_bytes:<5} {attempts:>9}")
    print(f"{'-'*80}")
    total_same = total_bytes - total_diff
    print(f"Bytes differ: {total_diff}/{total_bytes} "
          f"({100*total_diff/total_bytes:.1f}% different, "
          f"{total_same} bytes shared)")
    print(f"{'='*80}")
    print(f"\nWrote: {args.output_rom}")

    if args.plot and plot_data:
        try:
            import matplotlib.pyplot as plt
            fig, axes = plt.subplots(len(plot_data), 2,
                                     figsize=(14, 3 * len(plot_data)))
            if len(plot_data) == 1:
                axes = axes.reshape(1, -1)
            for i, (nm, target, evolved) in enumerate(plot_data):
                axes[i, 0].plot(target, linewidth=0.5, label="Original")
                axes[i, 0].plot(evolved, linewidth=0.5, alpha=0.7,
                                label="Evolved")
                axes[i, 0].set_title(f"{nm} - Waveform")
                axes[i, 0].legend(fontsize=8)
                axes[i, 1].plot(np.abs(target.astype(int) - evolved.astype(int)),
                                linewidth=0.5, color="red")
                axes[i, 1].set_title(f"{nm} - |difference|")
            plt.tight_layout()
            plt.savefig("evolved_rhythm_comparison.png", dpi=150)
            print("Saved: evolved_rhythm_comparison.png")
            plt.show()
        except ImportError:
            print("matplotlib not installed, skipping plots")


if __name__ == "__main__":
    main()
