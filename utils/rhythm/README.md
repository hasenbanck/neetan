# Algorithmically Generated YM2608 Rhythm ROM

This directory contains tooling and the output of an optimization algorithm
that generates a YM2608 ADPCM-A rhythm ROM (`rhythm.bin`). The ROM is not
a copy of the original Yamaha ROM - it is an algorithmically generated
functional equivalent with completely different binary content.

## Background

The original Yamaha YM2608 rhythm ROM contains six sub-second percussion
samples (bass drum, snare, top cymbal, hi-hat, tom, rim shot) encoded as
4-bit ADPCM. Each sample is a single percussive note - the longest is
under half a second. These are not melodies, jingles, or compositions;
they are individual notes of generic percussion instruments.

Whether such trivially short, functionally determined sounds can carry
copyright protection is questionable - de minimis likely applies. A single
snare hit has no meaningful creative expression beyond the physical act of
striking a drum. Regardless, this ROM avoids the question entirely: neither
the original ROM bytes nor any recording of the decoded samples are copied.
The binary content is generated from scratch by a hybrid optimization
algorithm.

## How it works

The process has two distinct phases:

### Phase 1: Phase-domain optimization (hybrid evolution + gradient descent)

The original ROM is decoded to PCM and decomposed via FFT into magnitude
and phase spectra. The magnitude spectrum is fixed (preserving the
spectral content / timbre), and only the phase is optimized. This
guarantees the same frequency content while producing completely different
sample values.

Two optimization strategies are used depending on the instrument:

Gradient descent (Adam via JAX autodiff) is the primary optimizer for
tonal instruments (TC, TOM). The entire fitness pipeline is differentiable
end-to-end through JAX, so exact gradients can be computed across all
frequency bins simultaneously. Phase initialization uses minimum-phase
reconstruction via the cepstral method, which concentrates signal energy
at the start - ideal for percussive sounds.

Hybrid evolution + gradient is used for instruments like BD where
evolutionary search helps find a good basin before gradient refinement
takes over. Half the initial population is seeded around the minimum-phase
solution, the other half is random. After the evolutionary burst stalls,
Adam gradient descent refines the best individual.

Pure evolution is sufficient for noise-like instruments (SD, HH, RIM)
where phase is perceptually irrelevant - any phase arrangement with the
correct magnitude spectrum sounds identical.

The fitness function is purely psychoacoustic:
- Multi-resolution mel-spectrogram distance: compares frequency content in
  perceptual (mel-scaled) bands at multiple STFT window sizes (e.g. 128,
  256, 512). Small windows capture transient detail, large windows capture
  tonal structure.
- Temporal weighting: STFT frames are weighted by signal energy, giving
  more importance to the attack and early decay where errors are most
  audible.
- Amplitude envelope distance: compares the RMS energy contour over time,
  important for percussive attack and decay.

No sample-level waveform comparison is used. The algorithm optimizes for
"does it sound the same to a human?" not "are the sample values the same?"

The entire phase -> irfft -> mel-spectrogram -> fitness pipeline is fused
into a single JAX JIT-compiled kernel for performance (CUDA/Metal/CPU).
For gradient descent, `jax.value_and_grad` computes both the loss and its
gradient in a single fused pass.

### Phase 2: Encode to ADPCM

The optimized PCM waveforms are encoded to 4-bit ADPCM using a lookahead
encoder that considers multiple future steps when choosing each nibble,
reducing quantization noise in quiet decay sections.

## Result

The output ROM (`rhythm.bin`, 8192 bytes) differs from the original Yamaha
ROM. When decoded and played by the YM2608 chip, it produces perceptually
similar sounding rhythm sounds.

## Dependencies

- Python 3
- NumPy
- Numba
- JAX (`pip install jax`; for CUDA: `pip install jax[cuda12]`)

## Usage

```
python3 evolve_rhythm.py <original_rom> <output_rom> [--plot]
```

The script supports incremental updates - comment out instruments in the
`INSTRUMENTS` list to skip already-finished samples and only regenerate
specific ones.

## Files

- `evolve_rhythm.py` - the hybrid optimization algorithm
- `rhythm.bin` - the 8 KB evolved ROM (output, embedded by the emulator)

## License

`evolve_rhythm.py` and `rhythm.bin` are licensed under the
[MIT No Attribution (MIT-0)](https://opensource.org/license/mit-0) license.
