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

use crate::{
    mcu::McuState,
    mcu_timer::{FrtState, McuTimerState},
    pcm::PcmState,
    submcu::SubMcuState,
};

pub const UART_BUFFER_SIZE: usize = 8192;
pub const ROM1_SIZE: usize = 0x8000;
pub const ROM2_SIZE: usize = 0x80000;
pub const RAM_SIZE: usize = 0x400;
pub const SRAM_SIZE: usize = 0x8000;
pub const NVRAM_SIZE: usize = 0x8000;
pub const CARDRAM_SIZE: usize = 0x8000;

pub struct Sc55State {
    pub mcu: McuState,
    pub dev_register: [u8; 0x80],
    pub romset: i32,
    pub mcu_mk1: bool,
    pub mcu_cm300: bool,
    pub mcu_st: bool,
    pub mcu_jv880: bool,
    pub mcu_scb55: bool,
    pub mcu_sc155: bool,
    pub uart_write_ptr: u32,
    pub uart_read_ptr: u32,
    pub uart_buffer: [u8; UART_BUFFER_SIZE],

    pub rom1: [u8; ROM1_SIZE],
    pub rom2: Vec<u8>,
    pub rom2_mask: i32,
    pub ram: [u8; RAM_SIZE],
    pub sram: [u8; SRAM_SIZE],
    pub nvram: [u8; NVRAM_SIZE],
    pub cardram: [u8; CARDRAM_SIZE],

    pub ga_int: [i32; 8],
    pub ga_int_enable: i32,
    pub ga_int_trigger: i32,
    pub sw_pos: u8,
    pub io_sd: u8,
    pub adf_rd: i32,
    pub analog_end_time: u64,
    pub ssr_rd: i32,
    pub uart_rx_byte: u8,
    pub uart_rx_delay: u64,
    pub uart_tx_delay: u64,

    pub frt: [FrtState; 3],
    pub timer: McuTimerState,
    pub timer_cycles: u64,
    pub timer_tempreg: u8,

    pub pcm: PcmState,
    pub waverom1: Vec<u8>,
    pub waverom2: Vec<u8>,
    pub waverom3: Vec<u8>,
    pub waverom_card: Vec<u8>,
    pub waverom_exp: Vec<u8>,

    pub sm: SubMcuState,
    pub sm_rom: [u8; 4096],

    pub sm_ram: [u8; 128],
    pub sm_shared_ram: [u8; 192],
    pub sm_access: [u8; 0x18],
    pub sm_p0_dir: u8,
    pub sm_p1_dir: u8,
    pub sm_device_mode: [u8; 32],
    pub sm_cts: u8,
    pub sm_timer_cycles: u64,
    pub sm_timer_prescaler: u8,
    pub sm_timer_counter: u8,
    pub sm_uart_rx_gotbyte: u8,
    pub sm_uart_rx_byte: u8,
    pub sm_uart_rx_delay: u64,

    pub operand_type: u32,
    pub operand_ea: u16,
    pub operand_ep: u8,
    pub operand_size: u8,
    pub operand_reg: u8,
    pub operand_status: u8,
    pub operand_data: u16,
    pub opcode_extended: u8,

    pub render_output: Vec<f32>,
    pub render_frames_written: u32,
    pub render_frames_requested: u32,
}

impl Default for Sc55State {
    fn default() -> Self {
        Self {
            mcu: McuState::default(),
            dev_register: [0; 0x80],
            romset: 0,
            mcu_mk1: false,
            mcu_cm300: false,
            mcu_st: false,
            mcu_jv880: false,
            mcu_scb55: false,
            mcu_sc155: false,
            uart_write_ptr: 0,
            uart_read_ptr: 0,
            uart_buffer: [0; UART_BUFFER_SIZE],
            rom1: [0; ROM1_SIZE],
            rom2: vec![0; ROM2_SIZE],
            rom2_mask: ROM2_SIZE as i32 - 1,
            ram: [0; RAM_SIZE],
            sram: [0; SRAM_SIZE],
            nvram: [0; NVRAM_SIZE],
            cardram: [0; CARDRAM_SIZE],
            ga_int: [0; 8],
            ga_int_enable: 0,
            ga_int_trigger: 0,
            sw_pos: 3,
            io_sd: 0x00,
            adf_rd: 0,
            analog_end_time: 0,
            ssr_rd: 0,
            uart_rx_byte: 0,
            uart_rx_delay: 0,
            uart_tx_delay: 0,
            frt: [FrtState::default(); 3],
            timer: McuTimerState::default(),
            timer_cycles: 0,
            timer_tempreg: 0,
            pcm: PcmState::default(),
            waverom1: Vec::new(),
            waverom2: Vec::new(),
            waverom3: Vec::new(),
            waverom_card: Vec::new(),
            waverom_exp: Vec::new(),
            sm: SubMcuState::default(),
            sm_rom: [0; 4096],
            sm_ram: [0; 128],
            sm_shared_ram: [0; 192],
            sm_access: [0; 0x18],
            sm_p0_dir: 0,
            sm_p1_dir: 0,
            sm_device_mode: [0; 32],
            sm_cts: 0,
            sm_timer_cycles: 0,
            sm_timer_prescaler: 0,
            sm_timer_counter: 0,
            sm_uart_rx_gotbyte: 0,
            sm_uart_rx_byte: 0,
            sm_uart_rx_delay: 0,
            operand_type: 0,
            operand_ea: 0,
            operand_ep: 0,
            operand_size: 0,
            operand_reg: 0,
            operand_status: 0,
            operand_data: 0,
            opcode_extended: 0,
            render_output: Vec::new(),
            render_frames_written: 0,
            render_frames_requested: 0,
        }
    }
}
