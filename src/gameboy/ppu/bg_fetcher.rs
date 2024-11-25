use std::{cell::{Ref, RefCell}, collections::VecDeque, rc::Rc};

use crate::gameboy::{mmu::Mmu, ppu::{LcdControlFlag, Ppu}};


pub enum FetchMode {
    Background,
    Window
}

pub struct BgFetcher {
    mmu: Rc<RefCell<Mmu>>,
    pub mode: FetchMode,

    pub cycle: u8,
    pub tile_counter: u16,
    tile_data_addr: u16,
    tile_num: u16,

    reset_on_first_step_3: bool,

    low_data: u8,
    high_data: u8
}

impl BgFetcher {
    pub fn new(mmu: Rc<RefCell<Mmu>>) -> Self {
        Self {
            mmu,
            mode: FetchMode::Background,

            cycle: 0,
            tile_counter: 0,
            tile_data_addr: 0,
            tile_num: 0,

            reset_on_first_step_3: false,

            low_data: 0,
            high_data: 0
        }
    }

    pub fn reset(&mut self) {
        self.mode = FetchMode::Background;
        self.cycle = 0;
        self.tile_counter = 0;
        self.tile_data_addr = 0;
        self.tile_num = 0;

        self.reset_on_first_step_3 = false;

        self.low_data = 0;
        self.high_data = 0;
    }

    fn get_adjusted_tile_index(mmu: &Ref<Mmu>, addr: u16, signed_tile_index: bool) -> u16 {
        if signed_tile_index {
            let tile = mmu.gpu_vram[(addr - 0x8000) as usize] as i8 as i16;
            if tile >= 0 {
                tile as u16 + 256
            }
            else {
                256 - (tile.abs() as u16)
            }
        }
        else {
            mmu.gpu_vram[(addr - 0x8000) as usize] as u16
        }
    }

    fn get_bg_map_start_addr(ldlc_flags: u8) -> u16 {
        match (ldlc_flags & LcdControlFlag::BGTileMapAddress as u8) != 0 {
            true => 0x9C00,
            false => 0x9800
        }
    }

    fn get_window_map_start_addr(ldlc_flags: u8) -> u16 {
        match (ldlc_flags & LcdControlFlag::WindowTileMapAddress as u8) != 0 {
            true => 0x9C00,
            false => 0x9800
        }
    }
   
    pub fn tick(&mut self, pixel_fifo: &mut VecDeque<u8>, window_line_counter: u8) {
        self.cycle += 1;

        // https://gbdev.io/pandocs/#fifo-pixel-fetcher
        match self.cycle {
            0 | 1 | 3 | 5 | 7 => { } // NOP 

            2 => {
                // fetch tile number from tile map
                let mmu = (*self.mmu).borrow();
                let ldlc_flags = mmu.io[0x40];
                let scan_line = mmu.io[0x44];
                let scroll_x: u8 = mmu.io[0x43];
                let scroll_y = mmu.io[0x42];
                
                let signed_tile_addressing: bool = ldlc_flags & LcdControlFlag::BGAndWindowTileData as u8 == 0;

                let map_addr = match self.mode {
                    FetchMode::Background => {
                        let tile_y = scan_line.wrapping_add(scroll_y) / 8;
                        let base_tile_map_offset = tile_y as u16 * 32;

                        Self::get_bg_map_start_addr(ldlc_flags) + base_tile_map_offset +
                            (scroll_x.wrapping_add(self.tile_counter as u8 * 8) / 8) as u16
                    }

                    FetchMode::Window => {
                        let base_tile_map_offset = (window_line_counter as u16 / 8) * 32;
                        
                        Self::get_window_map_start_addr(ldlc_flags) + 
                            self.tile_counter + base_tile_map_offset
                    }
                };
                    

                self.tile_num = Self::get_adjusted_tile_index(
                    &mmu, 
                    map_addr,
                    signed_tile_addressing
                );
            }

            4 => {
                // fetch low byte tile data
                let mmu = (*self.mmu).borrow();

                let offset = match self.mode {
                    FetchMode::Background => {
                        let scan_line = mmu.io[0x44];
                        let scroll_y = mmu.io[0x42];

                        (scan_line.wrapping_add(scroll_y) & 7) * 2
                    }

                    FetchMode::Window => {
                        (window_line_counter & 7) * 2
                    }
                };

                self.tile_data_addr = 0x8000 + (self.tile_num * 16) + (offset as u16);
                self.low_data = mmu.gpu_vram[(self.tile_data_addr - 0x8000) as usize];
            }

            6 => {
                // fetch high byte
                // the first time we reach here we go back to step 1

                // lets just NOP the first 6 cycles of mode 3?
                // if !self.reset_on_first_step_3 {
                //     self.reset_on_first_step_3 = true;
                //     self.cycle = 0;
                // }

                let mmu = (*self.mmu).borrow();
                self.high_data = mmu.gpu_vram[(self.tile_data_addr + 1 - 0x8000) as usize];
            }

            8..=u8::MAX => {
                // push data to the fifo if there are <= 8 items in it
                if pixel_fifo.len() == 0 {
                    for i in 0..8 {
                        let bx = 7 - i;
                        let color_bit = ((self.low_data & (1 << bx)) >> bx) | 
                            ((self.high_data & (1 << bx)) >> bx) << 1;
                        pixel_fifo.push_back(color_bit);
                    }

                    self.cycle = 0;
                    self.tile_counter += 1;
                }
            }
        }
    }
}