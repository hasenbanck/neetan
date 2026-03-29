/*
 * Copyright (C) 2021, 2024 nukeykt
 *
 *  Redistribution and use of this code or any derivative works are permitted
 *  provided that the following conditions are met:
 *
 *   - Redistributions may not be sold, nor may they be used in a commercial
 *     product or activity.
 *
 *   - Redistributions that are modified from the original source must include the
 *     complete source code, including the source code for all components used by a
 *     binary built from the modified sources. However, as a special exception, the
 *     source code distributed need not include anything that is normally distributed
 *     (in either source or binary form) with the major components (compiler, kernel,
 *     and so on) of the operating system on which the executable runs, unless that
 *     component itself accompanies the executable.
 *
 *   - Redistributions must reproduce the above copyright notice, this list of
 *     conditions and the following disclaimer in the documentation and/or other
 *     materials provided with the distribution.
 *
 *  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
 *  AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 *  IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 *  ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE
 *  LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 *  CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 *  SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 *  INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 *  CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 *  ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 *  POSSIBILITY OF SUCH DAMAGE.
 */

use crate::{Sc55State, mcu::*};

pub const INTERRUPT_SOURCE_NMI: u32 = 0;
pub const INTERRUPT_SOURCE_IRQ0: u32 = 1; // GPINT
pub const INTERRUPT_SOURCE_IRQ1: u32 = 2;
pub const INTERRUPT_SOURCE_FRT0_OCIA: u32 = 4;
pub const INTERRUPT_SOURCE_FRT0_OCIB: u32 = 5;
pub const INTERRUPT_SOURCE_FRT0_FOVI: u32 = 6;
pub const INTERRUPT_SOURCE_FRT1_OCIA: u32 = 8;
pub const INTERRUPT_SOURCE_FRT1_OCIB: u32 = 9;
pub const INTERRUPT_SOURCE_FRT1_FOVI: u32 = 10;
pub const INTERRUPT_SOURCE_FRT2_OCIA: u32 = 12;
pub const INTERRUPT_SOURCE_FRT2_OCIB: u32 = 13;
pub const INTERRUPT_SOURCE_FRT2_FOVI: u32 = 14;
pub const INTERRUPT_SOURCE_TIMER_CMIA: u32 = 15;
pub const INTERRUPT_SOURCE_TIMER_CMIB: u32 = 16;
pub const INTERRUPT_SOURCE_TIMER_OVI: u32 = 17;
pub const INTERRUPT_SOURCE_ANALOG: u32 = 18;
pub const INTERRUPT_SOURCE_UART_RX: u32 = 19;
pub const INTERRUPT_SOURCE_UART_TX: u32 = 20;
pub const INTERRUPT_SOURCE_MAX: u32 = 21;

pub const EXCEPTION_SOURCE_ADDRESS_ERROR: i32 = 0;
pub const EXCEPTION_SOURCE_INVALID_INSTRUCTION: i32 = 1;
pub const EXCEPTION_SOURCE_TRACE: i32 = 2;

fn mcu_interrupt_start(state: &mut Sc55State, mask: i32) {
    let pc = state.mcu.pc;
    mcu_push_stack(state, pc);
    let cp = state.mcu.cp as u16;
    mcu_push_stack(state, cp);
    let sr = state.mcu.sr;
    mcu_push_stack(state, sr);
    state.mcu.sr &= !STATUS_T;
    if mask >= 0 {
        state.mcu.sr &= !STATUS_INT_MASK;
        state.mcu.sr |= (mask as u16) << 8;
    }
    state.mcu.sleep = 0;
}

pub(crate) fn mcu_interrupt_set_request(state: &mut Sc55State, interrupt: u32, value: u32) {
    state.mcu.interrupt_pending[interrupt as usize] = value as u8;
}

pub(crate) fn mcu_interrupt_exception(state: &mut Sc55State, exception: u32) {
    state.mcu.exception_pending = exception as i32;
}

pub(crate) fn mcu_interrupt_trapa(state: &mut Sc55State, vector: u32) {
    state.mcu.trapa_pending[vector as usize] = 1;
}

fn mcu_interrupt_start_vector(state: &mut Sc55State, vector: u32, mask: i32) {
    let address = mcu_get_vector_address(state, vector);
    mcu_interrupt_start(state, mask);
    state.mcu.cp = (address >> 16) as u8;
    state.mcu.pc = address as u16;
}

pub(crate) fn mcu_interrupt_handle(state: &mut Sc55State) {
    for i in 0..16u32 {
        if state.mcu.trapa_pending[i as usize] != 0 {
            state.mcu.trapa_pending[i as usize] = 0;
            mcu_interrupt_start_vector(state, VECTOR_TRAPA_0 + i, -1);
            return;
        }
    }
    if state.mcu.exception_pending >= 0 {
        match state.mcu.exception_pending {
            EXCEPTION_SOURCE_ADDRESS_ERROR => {
                mcu_interrupt_start_vector(state, VECTOR_ADDRESS_ERROR, -1);
            }
            EXCEPTION_SOURCE_INVALID_INSTRUCTION => {
                mcu_interrupt_start_vector(state, VECTOR_INVALID_INSTRUCTION, -1);
            }
            EXCEPTION_SOURCE_TRACE => {
                mcu_interrupt_start_vector(state, VECTOR_TRACE, -1);
            }
            _ => {}
        }
        state.mcu.exception_pending = -1;
        return;
    }
    if state.mcu.interrupt_pending[INTERRUPT_SOURCE_NMI as usize] != 0 {
        // mcu.interrupt_pending[INTERRUPT_SOURCE_NMI] = 0;
        mcu_interrupt_start_vector(state, VECTOR_NMI, 7);
        return;
    }
    let mask = (state.mcu.sr >> 8) & 7;
    for i in (INTERRUPT_SOURCE_NMI + 1)..INTERRUPT_SOURCE_MAX {
        let mut vector: i32 = -1;
        let mut level: i32 = 0;
        if state.mcu.interrupt_pending[i as usize] == 0 {
            continue;
        }
        match i {
            INTERRUPT_SOURCE_IRQ0 => {
                if (state.dev_register[DEV_P1CR] & 0x20) == 0 {
                    continue;
                }
                vector = VECTOR_IRQ0 as i32;
                level = ((state.dev_register[DEV_IPRA] >> 4) & 7) as i32;
            }
            INTERRUPT_SOURCE_IRQ1 => {
                if (state.dev_register[DEV_P1CR] & 0x40) == 0 {
                    continue;
                }
                vector = VECTOR_IRQ1 as i32;
                level = (state.dev_register[DEV_IPRA] & 7) as i32;
            }
            INTERRUPT_SOURCE_FRT0_OCIA => {
                vector = VECTOR_INTERNAL_INTERRUPT_94 as i32;
                level = ((state.dev_register[DEV_IPRB] >> 4) & 7) as i32;
            }
            INTERRUPT_SOURCE_FRT0_OCIB => {
                vector = VECTOR_INTERNAL_INTERRUPT_98 as i32;
                level = ((state.dev_register[DEV_IPRB] >> 4) & 7) as i32;
            }
            INTERRUPT_SOURCE_FRT0_FOVI => {
                vector = VECTOR_INTERNAL_INTERRUPT_9C as i32;
                level = ((state.dev_register[DEV_IPRB] >> 4) & 7) as i32;
            }
            INTERRUPT_SOURCE_FRT1_OCIA => {
                vector = VECTOR_INTERNAL_INTERRUPT_A4 as i32;
                level = (state.dev_register[DEV_IPRB] & 7) as i32;
            }
            INTERRUPT_SOURCE_FRT1_OCIB => {
                vector = VECTOR_INTERNAL_INTERRUPT_A8 as i32;
                level = (state.dev_register[DEV_IPRB] & 7) as i32;
            }
            INTERRUPT_SOURCE_FRT1_FOVI => {
                vector = VECTOR_INTERNAL_INTERRUPT_AC as i32;
                level = (state.dev_register[DEV_IPRB] & 7) as i32;
            }
            INTERRUPT_SOURCE_FRT2_OCIA => {
                vector = VECTOR_INTERNAL_INTERRUPT_B4 as i32;
                level = ((state.dev_register[DEV_IPRC] >> 4) & 7) as i32;
            }
            INTERRUPT_SOURCE_FRT2_OCIB => {
                vector = VECTOR_INTERNAL_INTERRUPT_B8 as i32;
                level = ((state.dev_register[DEV_IPRC] >> 4) & 7) as i32;
            }
            INTERRUPT_SOURCE_FRT2_FOVI => {
                vector = VECTOR_INTERNAL_INTERRUPT_BC as i32;
                level = ((state.dev_register[DEV_IPRC] >> 4) & 7) as i32;
            }
            INTERRUPT_SOURCE_TIMER_CMIA => {
                vector = VECTOR_INTERNAL_INTERRUPT_C0 as i32;
                level = (state.dev_register[DEV_IPRC] & 7) as i32;
            }
            INTERRUPT_SOURCE_TIMER_CMIB => {
                vector = VECTOR_INTERNAL_INTERRUPT_C4 as i32;
                level = (state.dev_register[DEV_IPRC] & 7) as i32;
            }
            INTERRUPT_SOURCE_TIMER_OVI => {
                vector = VECTOR_INTERNAL_INTERRUPT_C8 as i32;
                level = (state.dev_register[DEV_IPRC] & 7) as i32;
            }
            INTERRUPT_SOURCE_ANALOG => {
                vector = VECTOR_INTERNAL_INTERRUPT_E0 as i32;
                level = (state.dev_register[DEV_IPRD] & 7) as i32;
            }
            INTERRUPT_SOURCE_UART_RX => {
                vector = VECTOR_INTERNAL_INTERRUPT_D4 as i32;
                level = ((state.dev_register[DEV_IPRD] >> 4) & 7) as i32;
            }
            INTERRUPT_SOURCE_UART_TX => {
                vector = VECTOR_INTERNAL_INTERRUPT_D8 as i32;
                level = ((state.dev_register[DEV_IPRD] >> 4) & 7) as i32;
            }
            _ => {}
        }

        if (mask as i32) < level {
            // mcu.interrupt_pending[INTERRUPT_SOURCE_NMI] = 0;
            mcu_interrupt_start_vector(state, vector as u32, level);
            return;
        }
    }
}
