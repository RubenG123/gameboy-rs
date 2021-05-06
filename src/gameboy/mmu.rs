use super::{input::Input, interupt::Interupt};

const PALETTE: [u8; 4] = [
    255, 192, 196, 0
];

pub struct Mmu {
    pub interupts: Interupt,
    pub input: Input,

    rom_bank_0: [u8; 0x4000],
    rom_bank_1: [u8; 0x4000], // for now, just a static bank, but needs to be switchable?

    gpu_vram: [u8; 0x2000],
    ram_switchable: [u8; 0x2000],
    cart_ram: [u8; 0x2000],
    working_ram: [u8; 0x2000],

    pub io: [u8; 0x100],
    zero_page: [u8; 0x80],

    pub tileset: [[[u8; 8]; 8]; 384],
    pub bg_palette: [u8; 4]
}

impl Mmu {
    pub fn new() -> Self {
        let mut mmu = Self {
            interupts: Interupt::new(),
            input: Input::new(),

            rom_bank_0: [0; 0x4000],
            rom_bank_1: [0; 0x4000],
            gpu_vram: [0; 0x2000],
            ram_switchable: [0; 0x2000],
            cart_ram: [0; 0x2000],
            working_ram: [0; 0x2000],
            io: [0; 0x100],
            zero_page: [0; 0x80],

            // ppu
            tileset: [[[0; 8]; 8]; 384],
            bg_palette: [
                PALETTE[0], PALETTE[1], PALETTE[2], PALETTE[3]
            ],
        };

        // set up zero page mem
        mmu.write_byte(0xFF10, 0x80);
        mmu.write_byte(0xFF11, 0xBF);
        mmu.write_byte(0xFF12, 0xF3);
        mmu.write_byte(0xFF14, 0xBF);
        mmu.write_byte(0xFF16, 0x3F);
        mmu.write_byte(0xFF19, 0xBF);
        mmu.write_byte(0xFF1A, 0x7A);
        mmu.write_byte(0xFF1B, 0xFF);
        mmu.write_byte(0xFF1C, 0x9F);
        mmu.write_byte(0xFF1E, 0xBF);
        mmu.write_byte(0xFF20, 0xFF);
        mmu.write_byte(0xFF23, 0xBF);
        mmu.write_byte(0xFF24, 0x77);
        mmu.write_byte(0xFF25, 0xF3);
        mmu.write_byte(0xFF26, 0xF1);
        mmu.write_byte(0xFF40, 0x91);
        mmu.write_byte(0xFF47, 0xFC);
        mmu.write_byte(0xFF48, 0xFF);
        mmu.write_byte(0xFF49, 0xFF);

        mmu
    }

    fn update_tileset(&mut self, addr: u16) {
        let effective_addr = addr - 0x8000;
        // 384 maximum total tiles
        // 256 is mem spaces are set to overlap fully
        // each tile ocupies 16 bytes, therefore 16 address spaces
        
        let tile = effective_addr / 16;
        // let y = (effective_addr % 16) / 2; 
        let y = ((addr >> 1) & 7) as u8;

        for x in 0..8 {
            let bit_idx: u8 = 1 << (7 - x);

            let color_lower; 
            if self.gpu_vram[(addr & 0x1FFE) as usize] & bit_idx > 0 
                { color_lower = 1 } else { color_lower = 0 };

            let color_higher;
            if self.gpu_vram[((addr & 0x1FFE) + 1) as usize] & bit_idx > 0 
                { color_higher = 2 } else { color_higher = 0 };

            self.tileset[tile as usize][y as usize][x as usize] = color_lower + color_higher;
        }
    }

    pub fn write_rom_to_bank_0(&mut self, rom_data: &Vec<u8>) {
        for i in 0..self.rom_bank_0.len() {
            self.rom_bank_0[i] = rom_data[i];
        }
    }

    pub fn write_rom_to_bank_1(&mut self, rom_data: &Vec<u8>) {
        for i in 0..self.rom_bank_0.len() {
            self.rom_bank_1[i] = rom_data[0x4000 + i];
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr & 0xF000 {
            // bios / rom_bank_0
            0x0 => {               
                self.rom_bank_0[addr as usize]
            },

            // rom_bank_0
            0x1000 | 0x2000 | 0x3000 => {
                self.rom_bank_0[addr as usize]
            }

            // rom_bank_1
            0x4000 | 0x5000 | 0x6000 | 0x7000 => {
                self.rom_bank_1[(addr as usize) - 0x4000]
            }

            // vram
            0x8000 | 0x9000 => {
                self.gpu_vram[(addr - 0x8000) as usize]
            }
            
            // cart ram
            0xA000 | 0xB000 => {
                self.cart_ram[(addr - 0xA000) as usize]
            }
            
            // internal ram
            0xC000 | 0xD000 => {
                self.working_ram[(addr - 0xC000) as usize]
            }

            // 0xE000 to 0xFFxx is a mirror of the internal ram

            0xE000 => {
                self.working_ram[(addr - 0xE000) as usize]
            }

            0xF000 => {
                match addr & 0x0F00 {
                    0x0000 | 0x0100 | 0x0200 | 0x0300 | 0x0400 |
                    0x0500 | 0x0600 | 0x0700 | 0x0800 | 0x0900 |
                    0x0A00 | 0x0B00 | 0x0C00 | 0x0D00 => {
                        return self.working_ram[(addr - 0xE000) as usize];
                    },

                    // 0x0E00 => {
                    //     // if addr < 0xFEA0 {
                    //     //     // TODO: write to sprite attr mem? for now just write it to working mem?
                    //     //     return self.sprite_table[(addr - 0xFE00) as usize];
                    //     // }

                    //     // FEAO -> FEFF
                    //     // "Empty but usable for io"?
                    //     // Some just return here
                    //     return 0;
                    // },

                    0x0F00 => {
                        if addr == 0xFF00 {
                            return self.input.read_joyp();
                        }

                        if addr == 0xFF0F {
                            return self.interupts.flags
                        }
                      
                        else if addr == 0xFFFF {
                            return self.interupts.enable
                        }

                        else if addr >= 0xFF80 && addr <= 0xFFFE {
                            return self.zero_page[(addr - 0xFF80) as usize]
                        } 

                        else if addr >= 0xFF00 && addr <= 0xFF7F {
                            return self.io[(addr - 0xFF00) as usize]
                        } 
                        
                        else {
                            panic!("unhandled byte read from memory! Addr: {:#X}", addr);
                        }
                    },

                    _ => {
                        println!("Unhandled branch in read request for mem (0xFxxx): {:#X}", addr);
                        std::process::exit(0);
                    }
                }
            }

            _ => {
                panic!("Unhandled read at addr {:#06X}", addr);
            }
        }
    }

    pub fn write_byte(&mut self, addr: u16, val: u8) {
        match addr & 0xF000 {
            0x0000 | 0x1000 | 0x2000 | 0x3000 | 0x4000 |
            0x5000 | 0x6000 | 0x7000 => {
                // Do nothing, 0x000 to 0x7FFF is ROM
                // some games for some reason try to write to rom anyway?
            }

            // vram
            0x8000 | 0x9000 => {
                self.gpu_vram[(addr - 0x8000) as usize] = val;
                if addr < 0x97FF { 
                    self.update_tileset(addr); 
                }
            }

            0xA000 | 0xB000 => {
                self.cart_ram[(addr - 0xA000) as usize] = val;
            }

            0xC000 | 0xD000 => {
                self.working_ram[(addr - 0xC000) as usize] = val;
            },

            0xE000 => {
                self.working_ram[(addr - 0xE000) as usize] = val;
            },

            0xF000 => {
                match addr & 0x0F00 {
                    0x0000 | 0x0100 | 0x0200 | 0x0300 | 0x0400 |
                    0x0500 | 0x0600 | 0x0700 | 0x0800 | 0x0900 |
                    0x0A00 | 0x0B00 | 0x0C00 | 0x0D00 => {
                        self.working_ram[(addr - 0xE000) as usize] = val;
                    },

                    0x0E00 => {
                        // if addr < 0xFEA0 {
                        //     self.sprite_table[(addr - 0xFE00) as usize] = val;
                        //     self.update_sprite(addr - 0xFE00, val);
                        // }

                        // "Empty but usable for io"?
                        // Some just return here
                        return;
                    },

                    0x0F00 => {
                        if addr == 0xFF00 {
                            self.input.set_column_line(val);
                        }

                        else if addr >= 0xFF80 && addr <= 0xFFFE {
                            self.zero_page[(addr - 0xFF80) as usize] = val;
                        }
                        
                        else if addr == 0xFF04 {
                            self.io[0x04] = 0; // writing any val to 0xFF04 sets it to 0? 
                        }

                        else if addr == 0xFF0F {
                            self.interupts.flags = val;
                        }

                        else if addr == 0xFF44 {
                            // Do nothing, this is read only ?
                        }

                        else if addr == 0xFF46 {
                            // println!("0xFF46 was written too! Is this being handled correctly? (timing wise)");
                            let source_addr: u16 = (val as u16) << 8;

                            for i in 0..160 {
                                let src_val = self.read_byte(source_addr + i);
                                self.write_byte(0xFE00 + i, src_val);
                            }
                        }

                        else if addr == 0xFF47 {
                            for i in 0..4 {
                                self.bg_palette[i] = PALETTE[((val >> (i * 2)) & 3) as usize];
                            }
                        }

                        // else if addr == 0xFF48 {
                        //     for i in 0..4 {
                        //         self.sprite_palette[0][i] = PALETTE[((val >> (i * 2)) & 3) as usize];
                        //     }
                        // }

                        // else if addr == 0xFF49 {
                        //     for i in 0..4 {
                        //         self.sprite_palette[1][i] = PALETTE[((val >> (i * 2)) & 3) as usize];
                        //     }
                        // }

                        else if addr >= 0xFF00 && addr <= 0xFF7F {
                            self.io[(addr - 0xFF00) as usize] = val;
                        }
                        
                        else if addr == 0xFFFF {
                            self.interupts.enable = val;
                        } 
                        
                        else {
                            panic!("unhandled byte write to memory! Addr: {:#X} Val: {:#X}", addr, val);
                        }
                    },

                    _ => {
                        println!("Unhandled branch in write request for mem (0xFxxx): {:#X}, Val: {:#X}", addr, val);
                        std::process::exit(0);
                    }
                }
            },

            _ => {
                println!("Unhandled write request for mem address: {:#X}, Val: {:#X}", addr, val);
                std::process::exit(0);
            }
        }
    }

    pub fn read_word(&self, addr: u16) -> u16 {
        self.read_byte(addr) as u16 + ((self.read_byte(addr + 1) as u16) << 8)
    }

    pub fn write_word(&mut self, addr: u16, val: u16) {
        let lower_val: u8 = (val & 0x00FF) as u8;
        let higher_val: u8 = ((val & 0xFF00) >> 8) as u8;

        self.write_byte(addr, lower_val);
        self.write_byte(addr + 1, higher_val);
    }
}