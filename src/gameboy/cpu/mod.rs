use std::{cell::RefCell, fmt, rc::Rc};
use self::disassembler::{Instruction, InstructionStep, disassemble};
use super::mmu::Mmu;

pub mod disassembler;

enum Flag {
    Z = 0b10000000,
    N = 0b01000000, // N = last math op was subtract
    H = 0b00100000,
    C = 0b00010000
}

pub struct Cpu {
    mmu: Rc<RefCell<Mmu>>,

    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8, 
    h: u8,
    l: u8,

    pc: u16,
    sp: u16,

    operand8: u8,
    operand16: u16,
    temp_val8: u8,
    temp_val_16: u16,

    instruction: Option<Instruction>,
    machine_cycles_taken_for_current_step: u8
}

impl fmt::Debug for Cpu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cpu:")
            .field("af", &format!("{:#06X}", self.af()))
            .field("bc", &format!("{:#06X}", self.bc()))
            .field("de", &format!("{:#06X}", self.de()))
            .field("hl", &format!("{:#06X}", self.hl()))
            
            .field("sp", &format!("{:#06X}", self.sp))
            .field("pc", &format!("{:#06X}", self.pc))

            .finish()
    }
}

impl Cpu {
    pub fn new(mmu: Rc<RefCell<Mmu>>) -> Self {
        Self {
            mmu,

            a: 0x01,
            b: 0x00,
            c: 0x13,
            d: 0x00,
            e: 0xD8,
            f: 0xB0,
            h: 0x01,
            l: 0x4D,

            pc: 0x100,
            sp: 0xFFFE,

            operand8: 0,
            operand16: 0,
            temp_val8: 0,
            temp_val_16: 0,

            instruction: None,
            machine_cycles_taken_for_current_step: 0
        }
    }

    pub fn is_processing_instruction(&self) -> bool {
        self.instruction.is_some()
    }

    pub fn set_interrupt_instruction(&mut self, instruction: Instruction) {
        self.instruction = Some(instruction);
    }

    // FLAG FUNCS
    #[inline]
    fn set_flag(&mut self, flag: Flag) {
        self.f = self.f | flag as u8;
    }

    #[inline]
    fn clear_flag(&mut self, flag: Flag) {
        self.f = self.f & !(flag as u8); 
    }

    #[inline]
    fn is_flag_set(&self, flag: Flag) -> bool {
        self.f & (flag as u8) > 0
    }

    #[inline]
    fn set_flag_if_cond_else_clear(&mut self, cond: bool, flag: Flag) {
        if cond {
            self.set_flag(flag);
        } else {
            self.clear_flag(flag);
        }
    }

    #[inline]
    fn handle_zero_flag(&mut self, register: u8) {
        if register == 0 {
            self.set_flag(Flag::Z);
        } else {
            self.clear_flag(Flag::Z);
        }
    }

    fn inc(&mut self, val: u8) -> u8 {
        self.set_flag_if_cond_else_clear((val & 0x0F) == 0x0F, Flag::H);

        let result: u8 = val.wrapping_add(1);

        self.handle_zero_flag(result);
        self.clear_flag(Flag::N);
        result
    }

    fn cp(&mut self, val: u8) {
        self.set_flag_if_cond_else_clear(self.a == val, Flag::Z);
        self.set_flag_if_cond_else_clear(val > self.a, Flag::C);
        self.set_flag_if_cond_else_clear(
            (val & 0xF) > (self.a & 0x0F), 
            Flag::H
        );
        self.set_flag(Flag::N);
    }

    // ARITHMETIC
    fn dec(&mut self, val: u8) -> u8 {
        let result: u8 = val.wrapping_sub(1);

        self.set_flag_if_cond_else_clear((val & 0x0F) == 0, Flag::H);
        self.handle_zero_flag(result);
        self.set_flag(Flag::N);
        result
    }

    fn or(&mut self, val: u8) {
        self.a = self.a | val;

        self.handle_zero_flag(self.a);
        self.clear_flag(Flag::C);
        self.clear_flag(Flag::N);
        self.clear_flag(Flag::H);
    }

    fn xor(&mut self, val: u8) {
        self.a = self.a ^ val;

        self.handle_zero_flag(self.a);
        self.clear_flag(Flag::C);
        self.clear_flag(Flag::N);
        self.clear_flag(Flag::H);
    }

    fn and(&mut self, val: u8) {
        self.a = self.a & val;

        self.handle_zero_flag(self.a);
        self.clear_flag(Flag::C);
        self.clear_flag(Flag::N);
        self.set_flag(Flag::H);
    }

    fn add(&mut self, val1: u8, val2: u8) -> u8 {
        let result = val1.wrapping_add(val2);

        self.clear_flag(Flag::N);
        self.set_flag_if_cond_else_clear(
            val1.checked_add(val2).is_none(),
            Flag::C
        );
        self.handle_zero_flag(result);

        // TODO: check if this half carry check is correct!!!!!
        let overflow: u16 = val1 as u16 + val2 as u16;
        let is_half_carry = (((result & 0x0F) as u16) + (overflow & 0x0F)) > 0x0F;
        self.set_flag_if_cond_else_clear(is_half_carry, Flag::H);

        result
    }

    fn adc(&mut self, _val: u8) {
        let val = _val.wrapping_add(if self.is_flag_set(Flag::C) {1} else {0});
        let result = self.a.wrapping_add(val);

        let overflowed = self.a.checked_add(val).is_none();
        self.set_flag_if_cond_else_clear(overflowed, Flag::C);
        self.set_flag_if_cond_else_clear(val == self.a, Flag::Z);
        
        let is_half_carry = ((val & 0x0F) + (self.a & 0x0F)) > 0x0F;
        self.set_flag_if_cond_else_clear(is_half_carry, Flag::H);

        self.clear_flag(Flag::N);
        self.a = result;
    }

    fn sub(&mut self, val: u8) {
        self.set_flag_if_cond_else_clear(val > self.a, Flag::C);
        self.set_flag_if_cond_else_clear((val & 0x0F) > (self.a & 0x0F), Flag::H);

        self.a = self.a.wrapping_sub(val);

        self.handle_zero_flag(self.a);
        self.set_flag(Flag::N);
    }

    fn add_two_reg_u16(&mut self, val1: u16, val2: u16) -> u16 {
        let result = val1.wrapping_add(val2);

        self.clear_flag(Flag::N);

        let overflowed = val1.checked_add(val2).is_none(); // check for overflow
        self.set_flag_if_cond_else_clear(overflowed, Flag::C);

        // https://newbedev.com/game-boy-half-carry-flag-and-16-bit-instructions-especially-opcode-0xe8
        // TODO: check if this half carry check is correct!!!!!
        let half_carry_occured = ((val1 & 0x0F) + (val2 & 0x0F)) > 0x0F;
        self.set_flag_if_cond_else_clear(half_carry_occured, Flag::H);

        result
    }

    // 16bit REGISTER HELPER FUNCS

    fn af(&self) -> u16 {
        ((self.a as u16) << 8) | (self.f as u16)
    }

    fn bc(&self) -> u16 {
        ((self.b as u16) << 8) | (self.c as u16)
    }

    fn de(&self) -> u16 {
        ((self.d as u16) << 8) | (self.e as u16)
    }

    fn hl(&self) -> u16 {
        ((self.h as u16) << 8) | (self.l as u16)
    }

    fn set_af(&mut self, val: u16) {
        // a = high bits, f = low bits
        self.a = ((val & 0xFF00) >> 8) as u8;
        self.f = (val & 0x00FF) as u8; // not sure if we need to & here
    }

    fn set_bc(&mut self, val: u16) {
        self.b = ((val & 0xFF00) >> 8) as u8;
        self.c = (val & 0x00FF) as u8;
    }

    fn set_de(&mut self, val: u16) {
        self.d = ((val & 0xFF00) >> 8) as u8;
        self.e = (val & 0x00FF) as u8;
    }

    fn set_hl(&mut self, val: u16) {
        self.h = ((val & 0xFF00) >> 8) as u8;
        self.l = (val & 0x00FF) as u8;
    }

    fn fetch(&mut self) -> u8 {
        let op = (*self.mmu).borrow().read_byte(self.pc);
        self.pc += 1;
        op
    }

    // STACK FUNCTIONS

    pub(super) fn push_pc_to_stack(&mut self) {
        self.write_word_to_stack(self.pc);
    }

    fn write_word_to_stack(&mut self, val: u16) {
        self.sp -= 2;
        (*self.mmu).borrow_mut().write_word(self.sp, val);
    }

    fn read_word_from_stack(&mut self) -> u16 {
        let val: u16 = (*self.mmu).borrow().read_word(self.sp);
        self.sp += 2;
        val
    }

    // MISC

    pub(super) fn set_pc(&mut self, pc: u16) {
        self.pc = pc;
    }

    // CYCLE FUNCTIONS

    pub fn tick(&mut self) {
        if self.instruction.is_none() {
            let opcode = self.fetch();
            let instruction = disassemble(opcode);

            if false {
                let instr_human_readable = instruction.human_readable.clone();
                // instr_human_readable = instr_human_readable.replace("u8", format!("{:#04X}", self.operand8).as_ref());
                // instr_human_readable = instr_human_readable.replace("i8", format!("{:#04X}", self.operand8).as_ref());
                // instr_human_readable = instr_human_readable.replace("u16", format!("{:#06X}", self.operand16).as_ref());
                let s = format!("PC:{} OP:{:#04X} {}", self.pc - 1, opcode, instr_human_readable);
                println!("{}", s);
                // println!("{:?}\n", self);
            }

            self.machine_cycles_taken_for_current_step += 1;
            self.instruction = Some(instruction);
            return;
        }

        self.machine_cycles_taken_for_current_step += 1;
        if self.machine_cycles_taken_for_current_step < 4 {
            return;
        }

        self.machine_cycles_taken_for_current_step = 0;

        let step;
        {
            let instruction = self.instruction.as_mut().unwrap();
            step = instruction.steps.pop_front().unwrap();
        }

        match step {
            InstructionStep::Standard(func) => {    
                func(self);                        
                // get the next step if possible
                self.handle_next_step();
            }

            InstructionStep::Instant(_) | InstructionStep::InstantConditional(_) => 
                panic!("We just waited to exec an instant step, the logic is bricked?")
        }
    }

    fn handle_next_step(&mut self) {
        let instruction_step_peek;
        {
            let instruction = self.instruction.as_mut().unwrap();
            
            // was this the last step
            if instruction.steps.is_empty() {
                self.instruction = None;
                return;
            }

            // peek next step
            instruction_step_peek = instruction.steps.front().unwrap();
        }

        // TODO: re-write code the below to:
        // inside a loop, peek the next instruction, if instant, then pop and execute, else return
        // current seems logic seems to work fine though

        let mut instruction_step;
            match instruction_step_peek {
                InstructionStep::Instant(_) | InstructionStep::InstantConditional(_) => {
                    let instruction = self.instruction.as_mut().unwrap();
                    instruction_step = instruction.steps.pop_front().unwrap();
                }

                _ => return
            }

        // loop through the steps incase there are multiple instant steps
        // (there technically should never be as they could just be defined as one?)
        loop {
            match &instruction_step {
                InstructionStep::Instant(func) => {
                    func(self);
                }

                InstructionStep::InstantConditional(func) => {
                    let branch = func(self);
                    if !branch {
                        self.instruction = None;
                        return;
                    }
                }

                _ => break
            }

            let instruction = self.instruction.as_mut().unwrap();
            if instruction.steps.is_empty() {
                self.instruction = None;
                return;
            }

            instruction_step = instruction.steps.pop_front().unwrap();
        }

        // put the instruction pack into the queue, at the front
        self.instruction.as_mut().unwrap().steps.push_front(instruction_step);
    }
}
