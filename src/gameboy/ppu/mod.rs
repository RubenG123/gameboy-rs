use std::{borrow::Borrow, cell::{RefCell}, cmp::Ordering, collections::VecDeque, rc::Rc};
use self::{bg_fetcher::{FetchMode, BgFetcher}, sprite_fetcher::SpriteFetcher};

use super::{interupt::InterruptFlag, mmu::Mmu};

mod bg_fetcher;
mod sprite_fetcher;

pub struct Ppu {
    mmu: Rc<RefCell<Mmu>>,
    mode: PpuMode,
    pub frame_buffer: [u8; 160 * 144],

    fifo_sprite_buffer: VecDeque<Sprite>,
    fifo_sprite_buffer_peek: Option<Sprite>,

    window_internal_line_counter: u8,
    bg_fifo: VecDeque<u8>,
    sprite_fifo: VecDeque<FifoPixel>,

    bg_fetcher: BgFetcher,
    sprite_fetcher: SpriteFetcher,

    fifo_scx_skipped: u8,
    fifo_wx_skipped: u8,
    fifo_wy_ly_equal: bool,
    fifo_current_x: usize,
    fifo_sprite_fetch: bool,
    reset: bool,

    mode_clock_cycles: u64,
    line_clock_cycles: u64,
    frame_clock_cycles: u64,

    wy_ly_equality_latch: bool,

    pub draw_flag: bool,

    ly_153_early: bool,

    power_on_line_0: bool,

    last_mode_3_cycles: u64,
}

pub struct Sprite {
    y: u8,
    x: u8,
    tile_num: u16,

    sprite_palette: usize,
    xflip: bool,
    yflip: bool,
    belowbg: bool
}

pub struct FifoPixel {
    sprite_palette: usize,
    sprite_color_bit: u8,
    belowbg: bool
}

#[derive(Clone, Copy)]
pub enum PpuMode {
    HBlank = 0, // mode 0
    VBlank = 1, // mode 1
    OAM    = 2, // mode 2, initial state
    VRAM   = 3  // mode 3
}

impl PpuMode {
   pub fn from_u8(val: u8) -> Self {
       match val {
           0 => PpuMode::HBlank,
           1 => PpuMode::VBlank,
           2 => PpuMode::OAM,
           3 => PpuMode::VRAM,
           _ => panic!("Invalid PpuMode value when trying to parse value!")
       }
   }  
}

pub enum LcdControlFlag {
    // 1: on, 0: off
    BGEnable =             0b0000_0001, 

    // Display sprites (obj): 
    // 1: on 
    // 0: off
    OBJEnable =            0b0000_0010, 

    // Sprite Size: 
    // 0: 8x8 
    // 1: 8x16
    OBJSize =              0b0000_0100, 

    // Where the BG tiles are mapped: 0: 0x9800-0x9BFF, 1: 0x9C00-0x9FFF
    BGTileMapAddress =     0b0000_1000, 

    // Location of Tiles for BG and window: 
    // 0: 0x8800-0x97FF
    // 1: 0x8000-0x87FF (Same location as the sprites (OBJ) (They overlap))
    BGAndWindowTileData =  0b0001_0000,

    // Render window as part of the draw
    // 0: off
    // 1: on
    WindowEnable =         0b0010_0000,

    // where window tiles are mapped
    // 0: 0x9800-0x9BFF 
    // 1: 0x9C00-0x9FFF
    WindowTileMapAddress = 0b0100_0000,

    LCDDisplayEnable =     0b1000_0000
}

impl Ppu {
    pub fn new(mmu: Rc<RefCell<Mmu>>) -> Self {
        let bg_fetcher = BgFetcher::new(mmu.clone());
        let sprite_fetcher = SpriteFetcher::new(mmu.clone());

        Self {
            mmu,
            mode: PpuMode::OAM,
            frame_buffer: [0; 160 * 144],

            fifo_sprite_buffer: VecDeque::new(),
            fifo_sprite_buffer_peek: None,

            window_internal_line_counter: 0,
            bg_fifo: VecDeque::new(),
            sprite_fifo: VecDeque::new(),

            bg_fetcher,
            sprite_fetcher,

            fifo_scx_skipped: 0,
            fifo_wx_skipped: 0,
            fifo_current_x: 0,
            fifo_wy_ly_equal: false,
            fifo_sprite_fetch: false,
            reset: false,

            mode_clock_cycles: 0,
            line_clock_cycles: 0,
            frame_clock_cycles: 0,

            wy_ly_equality_latch: false,

            draw_flag: false,

            ly_153_early: false,

            power_on_line_0: true,

            last_mode_3_cycles: 0
        }
    }

    fn get_scan_line(&self) -> u8 {
        (*self.mmu).borrow().io[0x44]
    }

    fn inc_scan_line(&mut self) -> u8 {
        let current_scan_line = self.get_scan_line();
        self.set_scan_line(current_scan_line + 1);
        current_scan_line + 1
    }

    fn set_scan_line(&mut self, val: u8) {
        (*self.mmu).borrow_mut().io[0x44] = val;
    }

    fn set_mode_lcdc(&mut self, mode: PpuMode) {
        let mut mmu = (*self.mmu).borrow_mut();
        let mut lcdc = mmu.io[0x41];
        lcdc = (lcdc & 0b11111100) | (mode as u8);
        mmu.io[0x41] = lcdc;
    }

    fn get_lyc(&self) -> u8 {
        (*self.mmu).borrow().io[0x45]
    }

    fn check_ly_eq_lyc(&mut self) {
        if self.get_scan_line() == self.get_lyc() {
            let mut mmu = (self.mmu).borrow_mut();
            mmu.io[0x41] = mmu.io[0x41] | 0b0000_0100;
        } else {
            let mut mmu = (self.mmu).borrow_mut();
            mmu.io[0x41] = mmu.io[0x41] & !0b0000_0100;
        }
    }

    fn update_stat_irq_conditions(&mut self, src: String) {
        self.mmu.borrow_mut().update_stat_irq_conditions(src);
    }

    const STAT_CHANGE_OFFSET: u64 = 4;

    pub fn tick(&mut self) {
        {
            let mut mmu = (*self.mmu).borrow_mut();
            if mmu.io[0x40] >> 7 == 0 && !self.reset {
                self.mode_clock_cycles = 0;
                self.line_clock_cycles = 0;
                self.frame_clock_cycles = 0;
                self.mode = PpuMode::HBlank;
                self.frame_buffer = [220; 160 * 144];
                self.reset = true;
                mmu.io[0x44] = 0; // set ly to 0
                mmu.io[0x41] = mmu.io[0x41] & 0b11111100;
                return;
            }

            if mmu.io[0x40] & LcdControlFlag::LCDDisplayEnable as u8 == 0 && self.reset {
                return;
            }
        }

        if self.reset {
            self.reset = false;
            self.power_on_line_0 = true;
            
            // TODO:
            // Should this be 7? Then goes to 8 next line so we start on
            // cycle 8 which I think is correct.
            self.line_clock_cycles = 6;
            self.mode_clock_cycles = 6;
            self.frame_clock_cycles = 6;
            self.mode = PpuMode::OAM;

            {
                let mut mmu = (*self.mmu).borrow_mut();

                // So the mode in the stat flag should be zero after reset 
                // even though the ppu is actually in mode 2? 🤔
                mmu.io[0x41] = mmu.io[0x41] & 0b11111100;
                mmu.io[0x44] = 0;
            }

            self.check_ly_eq_lyc();
        }

        self.mode_clock_cycles += 1;
        self.line_clock_cycles += 1;
        self.frame_clock_cycles += 1;

        if self.power_on_line_0 {
            self.power_on_line_0 = !self.power_on_line_0_tick();
            self.update_stat_irq_conditions(String::from("power on lyc check"));
            return;
        }

        // https://robertovaccari.com/gameboy/LCD-refresh-diagram.png
        match self.mode {
            // 0
            PpuMode::HBlank => {
                if self.mode_clock_cycles == Self::STAT_CHANGE_OFFSET {
                    self.set_mode_lcdc(PpuMode::HBlank);

                    let mut mmu = self.mmu.borrow_mut();
                    mmu.lock_oam = false;
                    mmu.lock_vram = false;
                }

                if self.line_clock_cycles == 456 {
                    self.power_on_line_0 = false;
                    self.mode_clock_cycles = 0;
                    self.line_clock_cycles = 0;
                    
                    // incr the scan line
                    // if scanline is 144, go to VBlank
                    // otherwise go to OAM (mode 2)
                    let current_scan_line = self.inc_scan_line();

                    if current_scan_line == 144 {
                        self.mode = PpuMode::VBlank;

                        // notify safe draw
                        self.draw_flag = true;
                    }
                    else {
                        self.mode = PpuMode::OAM;
                        self.mmu.borrow_mut().lock_oam = true;
                    }
                }
            }

            // 1
            PpuMode::VBlank => {
                if self.mode_clock_cycles == Self::STAT_CHANGE_OFFSET {
                    (self.mmu).borrow_mut().interupts.request_interupt(InterruptFlag::VBlank);
                    self.set_mode_lcdc(PpuMode::VBlank);
                    self.check_ly_eq_lyc();

                    // IF STAT OAM BIT IS SET, STAT INTR FIRES ON LINE 144 AT VBLANK???
                    // WAT
                    let mut mmu = (*self.mmu).borrow_mut();
                    if mmu.io[0x44] == 144 && mmu.io[0x41] & 0b0010_0000 != 0 {
                        mmu.interupts.request_interupt(InterruptFlag::Stat);
                    }
                }

                let scan_line = self.get_scan_line();
                
                // 153 only lasts 4 cycles
                if scan_line == 153 && self.line_clock_cycles == 4 {
                    self.set_scan_line(0);
                    self.check_ly_eq_lyc();
                    self.ly_153_early = true;
                }

                if self.line_clock_cycles == 456 {
                    self.line_clock_cycles = 0;

                    
                    if scan_line < 153 && !self.ly_153_early {
                        self.inc_scan_line();
                        self.check_ly_eq_lyc();
                    } else {
                        self.ly_153_early = false;

                        // starting a new frame so lets reset our state
                        self.mode_clock_cycles = 0;
                        self.frame_clock_cycles = 0;
                        self.window_internal_line_counter = 0;

                        // wy == ly latch is reset in VBlank
                        self.wy_ly_equality_latch = false;
                        
                        self.mode = PpuMode::OAM;
                        self.mmu.borrow_mut().lock_oam = true;
                    }
                }
            },
            
            // 2
            PpuMode::OAM => {
                if self.mode_clock_cycles == 1 {
                    // handle wy_ly latch
                    let mut mmu = (*self.mmu).borrow_mut();
                    let window_y = mmu.io[0x4A];
                    let scan_line = mmu.io[0x44];
                    if !self.wy_ly_equality_latch {
                        self.wy_ly_equality_latch = window_y == scan_line;
                    }

                    mmu.io[0x41] = mmu.io[0x41] & !0b0000_0100;
                }

                if self.mode_clock_cycles == Self::STAT_CHANGE_OFFSET {
                    if !self.power_on_line_0 {
                        self.set_mode_lcdc(PpuMode::OAM);
                    }

                    self.check_ly_eq_lyc();
                }

                if self.mode_clock_cycles == 80 {
                    self.fifo_sprite_buffer.clear();
                    self.fifo_sprite_buffer_peek = None;

                    let mut sprites = Vec::new();

                    {
                        let mmu = (*self.mmu).borrow();
                        let ldlc_flags = mmu.io[0x40];
                        let sprite_size = if ldlc_flags & LcdControlFlag::OBJSize as u8 != 0 
                            {16} else {8};
                        let scan_line = mmu.io[0x44];

                        let mut i = 0;
                        while i < 40 && sprites.len() < 10 {
                            let sprite_addr = (i as usize) * 4;
                    
                            let sprite_y = mmu.sprite_table[sprite_addr];
                            let sprite_x = mmu.sprite_table[sprite_addr + 1];
                            let tile_num = 
                                (mmu.sprite_table[sprite_addr + 2] & 
                                (if sprite_size == 16 { 0xFE } else { 0xFF })) as u16;

                            let flags = mmu.sprite_table[sprite_addr + 3];
                            let sprite_palette: usize = if flags & (1 << 4) != 0 {1} else {0};
                            let xflip: bool = flags & (1 << 5) != 0;
                            let yflip: bool = flags & (1 << 6) != 0;
                            let belowbg: bool = flags & (1 << 7) != 0;

                            let sprite = Sprite {
                                x: sprite_x,
                                y: sprite_y,
                                tile_num,
                                sprite_palette,
                                xflip,
                                yflip,
                                belowbg
                            };

                            let cond1 = sprite_x > 0;
                            let cond2 = scan_line + 16 >= sprite_y;
                            let cond3 = scan_line as u16 + 16 < sprite_y as u16 + sprite_size;
                            
                            if cond1 && cond2 && cond3 {
                                sprites.push(sprite);
                            }

                            i += 1;
                        }
                    }

                    // sort sprite by x pos (via a stable sort)
                    sprites.sort_by(|a, b| {
                        if a.x < b.x {
                            Ordering::Less
                        } else if a.x > b.x {
                            Ordering::Greater
                        } else {
                            Ordering::Equal
                        }
                    });

                    for s in sprites {
                        self.fifo_sprite_buffer.push_back(s);
                    }

                    self.fifo_sprite_buffer_peek = self.fifo_sprite_buffer.pop_front();

                    self.bg_fetcher.reset();
                    self.sprite_fetcher.reset();

                    self.fifo_scx_skipped = 0;
                    self.fifo_wx_skipped = 0;
                    self.fifo_current_x = 0;
                    self.bg_fifo.clear();
                    self.sprite_fifo.clear();
                    self.fifo_wy_ly_equal = false;
                    self.fifo_sprite_fetch = false;

                    self.mode_clock_cycles = 0;
                    self.mode = PpuMode::VRAM;
                    self.mmu.borrow_mut().lock_vram = true;
                }
            }

            // 3
            PpuMode::VRAM => {
                // apparently the drawing happens here?
                // (when BG tiles and sprites are rendered)
                // 172-289 clock cycles?
                // for now lets just say 172

                if self.mode_clock_cycles == Self::STAT_CHANGE_OFFSET {
                    self.set_mode_lcdc(PpuMode::VRAM);
                }

                if self.fifo_tick_2() {
                    let ly = self.get_scan_line();
                    match ly {
                        64 => {
                            if self.mode_clock_cycles != self.last_mode_3_cycles {
                                println!("mode 3 cycles: t-{} m-{} diff-172: {}", 
                                    self.mode_clock_cycles, 
                                    self.mode_clock_cycles as f32 / 4.0,
                                    self.mode_clock_cycles as i32 - 172 as i32
                                );
                                self.last_mode_3_cycles = self.mode_clock_cycles;
                            }
                        }
                        
                        _ => { }
                    }

                    
                    self.mode_clock_cycles = 0;

                    self.mode = PpuMode::HBlank;
                }
            }
        }

        self.update_stat_irq_conditions(String::from(""));
    }

    fn power_on_line_0_tick(&mut self) -> bool {
        match self.line_clock_cycles {
            83 => {
                self.set_mode_lcdc(PpuMode::VRAM);
                self.mmu.borrow_mut().lock_vram = true;
            }

            257 => {
                self.mode = PpuMode::HBlank;
                self.set_mode_lcdc(PpuMode::HBlank);

                let mut mmu = self.mmu.borrow_mut();
                mmu.lock_oam = false;
                mmu.lock_vram = false;
            }


            456 => {
                self.power_on_line_0 = false;
                self.mode_clock_cycles = 0;
                self.line_clock_cycles = 0;

                self.inc_scan_line();

                self.mode = PpuMode::OAM;
                self.mmu.borrow_mut().lock_oam = true;

                return true;
            }

            _ => { } // NOP
        }

        false
    }

    fn fifo_tick_2(&mut self) -> bool {
        if self.mode_clock_cycles <= 5 {
            return false;
        }

        let mmu = (*self.mmu).borrow();
        
        let ldlc_flags = mmu.io[0x40];
        let scan_line = mmu.io[0x44];
        let scroll_x: u8 = mmu.io[0x43];

        // sprite handling
        if self.fifo_sprite_fetch {
            let sprite = self.fifo_sprite_buffer_peek.as_ref().unwrap();
            self.sprite_fetcher.tick(&mut self.sprite_fifo, &sprite);
            if self.sprite_fetcher.cycle == 6 {
                self.fifo_sprite_fetch = false;
                self.fifo_sprite_buffer_peek = self.fifo_sprite_buffer.pop_front();
            } else {
                return false;
            }
        }

        if ldlc_flags & LcdControlFlag::OBJEnable as u8 != 0 && self.fifo_sprite_buffer_peek.is_some() {
            let candidate_sprite = self.fifo_sprite_buffer_peek.as_ref().unwrap();

            // needs to be a while??
            // many sprites can be on the same x pos
            // ppu cannot stall whilst bg_fetcher is busy, the fetcher is considered busy during
            // the first 5 t cycles of its period. 
            if candidate_sprite.x as usize <= self.fifo_current_x + 8 {
                if self.bg_fetcher.cycle <= 5 || self.bg_fifo.len() == 0 {
                    // statll

                    let window_line_counter = self.window_internal_line_counter.wrapping_sub(1);
                    self.bg_fetcher.tick(&mut self.bg_fifo, window_line_counter);
                    return false;
                }

                // self.bg_fetcher.cycle = 1; // reset bg fetcher

                // pause pixel pushing and bg fetcher
                // and start sprite fetcher
                self.fifo_sprite_fetch = true; 
                self.sprite_fetcher.cycle = 0;
                return false;
            }
        }

        let window_line_counter = self.window_internal_line_counter.wrapping_sub(1);
        self.bg_fetcher.tick(&mut self.bg_fifo, window_line_counter);

        if self.bg_fifo.len() == 0 {
            return false;
        }

        let mut color_bit = self.bg_fifo.pop_front().unwrap();

        // scx skipping
        if !self.fifo_wy_ly_equal && self.fifo_scx_skipped < scroll_x & 7 {
            self.fifo_scx_skipped += 1;
            return false;
        }
        
        // bg enable check, if not, then color bit 0
        let bg_enabled = ldlc_flags & LcdControlFlag::BGEnable as u8 != 0;
        if !bg_enabled && !self.fifo_wy_ly_equal {
            color_bit = 0;
        }

        let mut color = mmu.bg_palette[color_bit as usize];

        // sprite checking
        let sprite_pixel = self.sprite_fifo.pop_front();
        if sprite_pixel.is_some() {
            let sprite_pixel = sprite_pixel.unwrap();

            let skip = (sprite_pixel.belowbg && color_bit != 0) || sprite_pixel.sprite_color_bit == 0;

            if !skip {
                color = mmu.sprite_palette[sprite_pixel.sprite_palette][sprite_pixel.sprite_color_bit as usize];
            }
        }

        let fb_offset = (scan_line as usize * 160) + self.fifo_current_x;
        self.frame_buffer[fb_offset] = color;
        self.fifo_current_x += 1;

        return self.fifo_current_x == 160 
    }

    // This will probably return a bool to say we've drawn the whole line,
    // so we know when to change ppu modes
    fn fifo_tick(&mut self) -> bool {
        // sprite fifo handling
        if self.fifo_sprite_fetch {
            let sprite = self.fifo_sprite_buffer_peek.as_ref().unwrap();
            self.sprite_fetcher.tick(&mut self.sprite_fifo, &sprite);
            if self.sprite_fetcher.cycle == 6 {
                self.fifo_sprite_fetch = false;
                self.fifo_sprite_buffer_peek = self.fifo_sprite_buffer.pop_front();
            } else {
                return false;
            }
        }

        let mmu = (*self.mmu).borrow();
        
        let ldlc_flags = mmu.io[0x40];
        let scan_line = mmu.io[0x44];
        let scroll_x: u8 = mmu.io[0x43];

        // check sprite
        if ldlc_flags & LcdControlFlag::OBJEnable as u8 != 0  && self.fifo_sprite_buffer_peek.is_some() {
            let candidate_sprite = self.fifo_sprite_buffer_peek.as_ref().unwrap();

            // needs to be a while??
            // many sprites can be on the same x pos
            // ppu cannot stall whilst bg_fetcher is busy, the fetcher is considered busy during
            // the first 5 t cycles of its period. 
            if candidate_sprite.x as usize <= self.fifo_current_x + 8 {
                if self.bg_fetcher.cycle <= 5 {
                    // statll
                    return false;
                }

                self.bg_fetcher.cycle = 1; // reset bg fetcher

                // pause pixel pushing and bg fetcher
                // and start sprite fetcher
                self.fifo_sprite_fetch = true; 
                self.sprite_fetcher.cycle = 0;
                return false;
            }
        }

        let wx = mmu.io[0x4B];
        let window_x: i16 = 
            if wx >= 7 {
                (wx - 7) as i16
            } else {
                (wx as i8 - 7) as i16
            };

        let window_enabled = ldlc_flags & LcdControlFlag::WindowEnable as u8 != 0;
        let start_drawing_window = 
            self.wy_ly_equality_latch && 
            window_x <= self.fifo_current_x as i16;

        // check if we need to switch bg/wd fifo to window mode
        if !self.fifo_wy_ly_equal && window_enabled && start_drawing_window {
            self.fifo_wy_ly_equal = true;
            self.window_internal_line_counter += 1;

            self.bg_fetcher.mode = FetchMode::Window;
            self.bg_fetcher.cycle = 0;
            self.bg_fetcher.tile_counter = 0;

            self.bg_fifo.clear();
        }

        let window_line_counter = self.window_internal_line_counter.wrapping_sub(1);
        self.bg_fetcher.tick(&mut self.bg_fifo, window_line_counter);
        if self.bg_fifo.len() == 0 { return false }

        let mut color_bit = self.bg_fifo.pop_front().unwrap();

        if !self.fifo_wy_ly_equal && self.fifo_scx_skipped < scroll_x & 7 {
            self.fifo_scx_skipped += 1;
            return false;
        }

        if self.fifo_wy_ly_equal && window_x < 0 && self.fifo_wx_skipped < 7 {
            if (window_x + self.fifo_wx_skipped as i16) < 0 {
                self.fifo_wx_skipped += 1;
                return false;
            }
        }

        // if bg isn't enabled, and we're drawing the bg, then set color bit to 0
        let bg_enabled = ldlc_flags & LcdControlFlag::BGEnable as u8 != 0;
        if !bg_enabled && !self.fifo_wy_ly_equal{
            color_bit = 0;
        }

        let mut color = mmu.bg_palette[color_bit as usize];

        let sprite_pixel = self.sprite_fifo.pop_front();
        if sprite_pixel.is_some() {
            let sprite_pixel = sprite_pixel.unwrap();

            let skip = (sprite_pixel.belowbg && color_bit != 0) || sprite_pixel.sprite_color_bit == 0;

            if !skip {
                color = mmu.sprite_palette[sprite_pixel.sprite_palette][sprite_pixel.sprite_color_bit as usize];
            }
        }

        let fb_offset = (scan_line as usize * 160) + self.fifo_current_x;

        self.frame_buffer[fb_offset] = color;

        self.fifo_current_x += 1;
        return self.fifo_current_x == 160 
    }
}
