use super::{Runtime, REG_LR, REG_PC};
use crate::arm7tdmi::{self, ShiftKind};

pub trait Arm7TdmiExecution {
    fn execute_arm_data_processing(&mut self, opcode: u8, rd: usize, rn: usize, rhs: u32, set_flags: bool);
    fn execute_arm_multiply(&mut self, rd: usize, rn: usize, rs: usize, rm: usize, accumulate: bool, set_flags: bool);
    fn execute_arm_multiply_long(&mut self, rd_hi: usize, rd_lo: usize, rs: usize, rm: usize, signed: bool, accumulate: bool, set_flags: bool);
    fn execute_arm_swap(&mut self, rd: usize, rn: usize, rm: usize, byte: bool);
    fn execute_arm_halfword_transfer(&mut self, load: bool, signed: bool, halfword: bool, rd: usize, rn: usize, offset: i32, pre_index: bool, up: bool, write_back: bool);
    fn execute_arm_single_transfer(&mut self, load: bool, byte: bool, rd: usize, rn: usize, offset: u32, pre_index: bool, up: bool, write_back: bool);
    fn execute_arm_block_transfer(&mut self, load: bool, rn: usize, register_list: u16, pre_index: bool, up: bool, write_back: bool, user_mode: bool);
    fn execute_arm_mrs(&mut self, rd: usize, spsr: bool);
    fn execute_arm_msr(&mut self, spsr: bool, field_mask: u8, value: u32);
    fn execute_arm_swi(&mut self, comment: u32);
    fn execute_thumb_move_shifted(&mut self, kind: u8, rd: usize, rs: usize, offset: u8);
    fn execute_thumb_add_sub(&mut self, sub: bool, rd: usize, lhs: usize, rhs: u32);
    fn execute_thumb_alu(&mut self, opcode: u8, rd: usize, rs: usize);
    fn execute_thumb_high_register(&mut self, op: u8, rd: usize, rs: usize);
    fn execute_thumb_load_store_register(&mut self, load: bool, byte: bool, rd: usize, rb: usize, ro: usize);
    fn execute_thumb_load_store_sign_half(&mut self, kind: u8, rd: usize, rb: usize, ro: usize);
    fn execute_thumb_load_store_immediate(&mut self, load: bool, byte: bool, rd: usize, rb: usize, offset: u8);
    fn execute_thumb_load_store_halfword(&mut self, load: bool, rd: usize, rb: usize, offset: u8);
    fn execute_thumb_sp_relative(&mut self, load: bool, rd: usize, offset: u8);
    fn execute_thumb_address(&mut self, rd: usize, use_sp: bool, word_offset: u8);
    fn execute_thumb_add_sp(&mut self, negative: bool, imm: u16);
    fn execute_thumb_push_pop(&mut self, load: bool, registers: u8, extra_lr_pc: bool);
    fn execute_thumb_multiple(&mut self, load: bool, rb: usize, register_list: u8);
    fn execute_thumb_swi(&mut self, comment: u8);
}

fn set_logic_flags(rt: &mut Runtime, value: u32, carry: Option<bool>) {
    let old = rt.nzcv();
    rt.set_flags(arm7tdmi::Nzcv::new(
        value & 0x8000_0000 != 0,
        value == 0,
        carry.unwrap_or(old.c),
        old.v,
    ));
}

fn operand_address(base: u32, offset: i32, up: bool) -> u32 {
    if up { base.wrapping_add(offset as u32) } else { base.wrapping_sub(offset as u32) }
}

impl Arm7TdmiExecution for Runtime {
    fn execute_arm_data_processing(&mut self, opcode: u8, rd: usize, rn: usize, rhs: u32, set_flags: bool) {
        let lhs = self.read_reg(rn);
        match opcode {
            0 => { let value = lhs & rhs; self.mov(rd, value, false); if set_flags { set_logic_flags(self, value, None); } }
            1 => { let value = lhs ^ rhs; self.mov(rd, value, false); if set_flags { set_logic_flags(self, value, None); } }
            2 => { let (value, flags) = arm7tdmi::sub_with_borrow(lhs, rhs, false); self.mov(rd, value, false); if set_flags { self.set_flags(flags); } }
            3 => { let (value, flags) = arm7tdmi::sub_with_borrow(rhs, lhs, false); self.mov(rd, value, false); if set_flags { self.set_flags(flags); } }
            4 => { self.add(rd, lhs, rhs, set_flags); }
            5 => { self.adc(rd, lhs, rhs, set_flags); }
            6 => { self.sbc(rd, lhs, rhs, set_flags); }
            7 => { let carry = self.nzcv().c; let (value, flags) = arm7tdmi::sub_with_borrow(rhs, lhs, !carry); self.mov(rd, value, false); if set_flags { self.set_flags(flags); } }
            8 => { let value = lhs & rhs; set_logic_flags(self, value, None); }
            9 => { let value = lhs ^ rhs; set_logic_flags(self, value, None); }
            10 => self.compare(lhs, rhs),
            11 => { let (_, flags) = arm7tdmi::add_with_carry(lhs, rhs, false); self.set_flags(flags); }
            12 => { let value = lhs | rhs; self.mov(rd, value, false); if set_flags { set_logic_flags(self, value, None); } }
            13 => { self.mov(rd, rhs, false); if set_flags { set_logic_flags(self, rhs, None); } }
            14 => { let value = lhs & !rhs; self.mov(rd, value, false); if set_flags { set_logic_flags(self, value, None); } }
            15 => { let value = !rhs; self.mov(rd, value, false); if set_flags { set_logic_flags(self, value, None); } }
            _ => self.unimplemented(self.read_reg(REG_PC), opcode as u32, "ARM data-processing"),
        }
    }

    fn execute_arm_multiply(&mut self, rd: usize, rn: usize, rs: usize, rm: usize, accumulate: bool, set_flags: bool) {
        let mut result = self.read_reg(rm).wrapping_mul(self.read_reg(rs));
        if accumulate { result = result.wrapping_add(self.read_reg(rn)); }
        self.write_reg(rd, result);
        if set_flags { let old = self.nzcv(); self.set_flags(arm7tdmi::Nzcv::new(result & 0x8000_0000 != 0, result == 0, old.c, old.v)); }
    }

    fn execute_arm_multiply_long(&mut self, rd_hi: usize, rd_lo: usize, rs: usize, rm: usize, signed: bool, accumulate: bool, set_flags: bool) {
        let value = if signed {
            (self.read_reg(rm) as i32 as i64).wrapping_mul(self.read_reg(rs) as i32 as i64) as u64
        } else {
            (self.read_reg(rm) as u64).wrapping_mul(self.read_reg(rs) as u64)
        };
        let mut result = value;
        if accumulate { result = result.wrapping_add((u64::from(self.read_reg(rd_hi)) << 32) | u64::from(self.read_reg(rd_lo))); }
        self.write_reg(rd_lo, result as u32);
        self.write_reg(rd_hi, (result >> 32) as u32);
        if set_flags { let old = self.nzcv(); self.set_flags(arm7tdmi::Nzcv::new(result >> 63 != 0, result == 0, old.c, old.v)); }
    }

    fn execute_arm_swap(&mut self, rd: usize, rn: usize, rm: usize, byte: bool) {
        let address = self.read_reg(rn);
        if byte {
            let old = self.read8(address);
            self.write8(address, self.read_reg(rm) as u8);
            self.write_reg(rd, old as u32);
        } else {
            let old = self.read32(address);
            self.write32(address & !3, self.read_reg(rm));
            self.write_reg(rd, old);
        }
    }

    fn execute_arm_halfword_transfer(&mut self, load: bool, signed: bool, halfword: bool, rd: usize, rn: usize, offset: i32, pre_index: bool, up: bool, write_back: bool) {
        let base = self.read_reg(rn);
        let effective = if pre_index { operand_address(base, offset, up) } else { base };
        if load {
            let value = if halfword {
                let raw = self.read16(effective) as u32;
                if signed && raw & 0x8000 != 0 { raw | 0xffff_0000 } else { raw }
            } else {
                let raw = self.read8(effective) as u32;
                if signed && raw & 0x80 != 0 { raw | 0xffff_ff00 } else { raw }
            };
            self.write_reg(rd, value);
        } else {
            self.write16(effective, self.read_reg(rd) as u16);
        }
        if write_back || !pre_index { self.write_reg(rn, operand_address(base, offset, up)); }
    }

    fn execute_arm_single_transfer(&mut self, load: bool, byte: bool, rd: usize, rn: usize, offset: u32, pre_index: bool, up: bool, write_back: bool) {
        let base = self.read_reg(rn);
        let address = if pre_index { operand_address(base, offset as i32, up) } else { base };
        if load { self.write_reg(rd, if byte { self.read8(address) as u32 } else { self.read32(address) }); }
        else if byte { self.write8(address, self.read_reg(rd) as u8); } else { self.write32(address & !3, self.read_reg(rd)); }
        if write_back || !pre_index { self.write_reg(rn, operand_address(base, offset as i32, up)); }
    }

    fn execute_arm_block_transfer(&mut self, load: bool, rn: usize, register_list: u16, pre_index: bool, up: bool, write_back: bool, _user_mode: bool) {
        let count = register_list.count_ones();
        let base = self.read_reg(rn);
        let mut address = if up { base } else { base.wrapping_sub(count * 4) };
        if pre_index { address = if up { address.wrapping_add(4) } else { address.wrapping_sub(4) }; }
        for reg in 0..16usize {
            if register_list & (1u16 << reg) != 0 {
                if load { let value = self.read32(address); self.write_reg(reg, value); }
                else { self.write32(address & !3, self.read_reg(reg)); }
                address = address.wrapping_add(4);
            }
        }
        if write_back { self.write_reg(rn, if up { base.wrapping_add(count * 4) } else { base.wrapping_sub(count * 4) }); }
    }

    fn execute_arm_mrs(&mut self, rd: usize, _spsr: bool) {
        self.write_reg(rd, self.cpu.cpsr);
    }

    fn execute_arm_msr(&mut self, spsr: bool, field_mask: u8, value: u32) {
        if spsr { return; }
        let mut cpsr = self.cpu.cpsr;
        if field_mask & 1 != 0 { cpsr = (cpsr & !0x0000_00ff) | (value & 0x0000_00ff); }
        if field_mask & 2 != 0 { cpsr = (cpsr & !0x0000_ff00) | (value & 0x0000_ff00); }
        if field_mask & 4 != 0 { cpsr = (cpsr & !0x00ff_0000) | (value & 0x00ff_0000); }
        if field_mask & 8 != 0 { cpsr = (cpsr & !0xff00_0000) | (value & 0xff00_0000); }
        self.cpu.cpsr = cpsr;
        self.cpu.thumb = cpsr & (1 << 5) != 0;
    }

    fn execute_arm_swi(&mut self, comment: u32) { self.halt_with_exception("SWI", comment); }

    fn execute_thumb_move_shifted(&mut self, kind: u8, rd: usize, rs: usize, offset: u8) {
        let shift = match kind { 0 => ShiftKind::Lsl, 1 => ShiftKind::Lsr, _ => ShiftKind::Asr };
        let result = self.shift(self.read_reg(rs), shift, offset, false);
        self.write_reg(rd, result.value);
        self.set_flags(arm7tdmi::Nzcv::new(result.value & 0x8000_0000 != 0, result.value == 0, result.carry, self.nzcv().v));
    }

    fn execute_thumb_add_sub(&mut self, sub: bool, rd: usize, lhs: usize, rhs: u32) {
        if sub { self.sub(rd, self.read_reg(lhs), rhs, true); } else { self.add(rd, self.read_reg(lhs), rhs, true); }
    }

    fn execute_thumb_alu(&mut self, opcode: u8, rd: usize, rs: usize) {
        let lhs = self.read_reg(rd);
        let rhs = self.read_reg(rs);
        match opcode {
            0 => { let v = lhs & rhs; self.mov(rd, v, false); set_logic_flags(self, v, None); }
            1 => { let v = lhs ^ rhs; self.mov(rd, v, false); set_logic_flags(self, v, None); }
            2 => { let r = self.shift(lhs, ShiftKind::Lsl, (rhs & 0xff) as u8, true); self.write_reg(rd, r.value); set_logic_flags(self, r.value, r.carry.into()); }
            3 => { let r = self.shift(lhs, ShiftKind::Lsr, (rhs & 0xff) as u8, true); self.write_reg(rd, r.value); set_logic_flags(self, r.value, r.carry.into()); }
            4 => { let r = self.shift(lhs, ShiftKind::Asr, (rhs & 0xff) as u8, true); self.write_reg(rd, r.value); set_logic_flags(self, r.value, r.carry.into()); }
            5 => self.adc(rd, lhs, rhs, true),
            6 => self.sbc(rd, lhs, rhs, true),
            7 => { let r = self.shift(lhs, ShiftKind::Ror, (rhs & 0xff) as u8, true); self.write_reg(rd, r.value); set_logic_flags(self, r.value, r.carry.into()); }
            8 => { let v = lhs & rhs; set_logic_flags(self, v, None); }
            9 => { let (v, f) = arm7tdmi::sub_with_borrow(0, rhs, false); self.write_reg(rd, v); self.set_flags(f); }
            10 => self.compare(lhs, rhs),
            11 => { let (_, f) = arm7tdmi::add_with_carry(lhs, rhs, false); self.set_flags(f); }
            12 => { let v = lhs | rhs; self.mov(rd, v, false); set_logic_flags(self, v, None); }
            13 => { self.write_reg(rd, lhs.wrapping_mul(rhs)); let old = self.nzcv(); let v = self.read_reg(rd); self.set_flags(arm7tdmi::Nzcv::new(v & 0x8000_0000 != 0, v == 0, old.c, old.v)); }
            14 => { let v = lhs & !rhs; self.mov(rd, v, false); set_logic_flags(self, v, None); }
            15 => { let v = !rhs; self.mov(rd, v, false); set_logic_flags(self, v, None); }
            _ => unreachable!(),
        }
    }

    fn execute_thumb_high_register(&mut self, op: u8, rd: usize, rs: usize) {
        match op {
            0 => self.write_reg(rd, self.read_reg(rd).wrapping_add(self.read_reg(rs))),
            1 => self.compare(self.read_reg(rd), self.read_reg(rs)),
            2 => self.write_reg(rd, self.read_reg(rs)),
            _ => self.dispatch_exchange(self.read_reg(rs)),
        }
    }

    fn execute_thumb_load_store_register(&mut self, load: bool, byte: bool, rd: usize, rb: usize, ro: usize) {
        let address = self.read_reg(rb).wrapping_add(self.read_reg(ro));
        if load { self.write_reg(rd, if byte { self.read8(address) as u32 } else { self.read32(address) }); }
        else if byte { self.write8(address, self.read_reg(rd) as u8); } else { self.write32(address & !3, self.read_reg(rd)); }
    }

    fn execute_thumb_load_store_sign_half(&mut self, kind: u8, rd: usize, rb: usize, ro: usize) {
        let address = self.read_reg(rb).wrapping_add(self.read_reg(ro));
        match kind {
            0 => self.write16(address, self.read_reg(rd) as u16),
            1 => self.write_reg(rd, self.read8(address) as u32),
            2 => { let v = self.read8(address) as u32; self.write_reg(rd, if v & 0x80 != 0 { v | 0xffff_ff00 } else { v }); }
            3 => { let v = self.read16(address) as u32; self.write_reg(rd, if v & 0x8000 != 0 { v | 0xffff_0000 } else { v }); }
            _ => self.unimplemented(address, kind as u32, "Thumb sign/halfword"),
        }
    }

    fn execute_thumb_load_store_immediate(&mut self, load: bool, byte: bool, rd: usize, rb: usize, offset: u8) {
        let scale = if byte { 1 } else { 4 };
        let address = self.read_reg(rb).wrapping_add(u32::from(offset) * scale);
        if load { self.write_reg(rd, if byte { self.read8(address) as u32 } else { self.read32(address) }); }
        else if byte { self.write8(address, self.read_reg(rd) as u8); } else { self.write32(address & !3, self.read_reg(rd)); }
    }

    fn execute_thumb_load_store_halfword(&mut self, load: bool, rd: usize, rb: usize, offset: u8) {
        let address = self.read_reg(rb).wrapping_add(u32::from(offset) * 2);
        if load { self.write_reg(rd, self.read16(address) as u32); } else { self.write16(address, self.read_reg(rd) as u16); }
    }

    fn execute_thumb_sp_relative(&mut self, load: bool, rd: usize, offset: u8) {
        let address = self.read_reg(13).wrapping_add(u32::from(offset) * 4);
        if load { self.write_reg(rd, self.read32(address)); } else { self.write32(address & !3, self.read_reg(rd)); }
    }

    fn execute_thumb_address(&mut self, rd: usize, use_sp: bool, word_offset: u8) {
        let base = if use_sp { self.read_reg(13) } else { self.read_reg(REG_PC) & !2 };
        self.write_reg(rd, base.wrapping_add(u32::from(word_offset) * 4));
    }

    fn execute_thumb_add_sp(&mut self, negative: bool, imm: u16) {
        let sp = self.read_reg(13);
        self.write_reg(13, if negative { sp.wrapping_sub(u32::from(imm)) } else { sp.wrapping_add(u32::from(imm)) });
    }

    fn execute_thumb_push_pop(&mut self, load: bool, registers: u8, extra_lr_pc: bool) {
        let count = registers.count_ones() + u32::from(extra_lr_pc);
        if load {
            let mut address = self.read_reg(13);
            for reg in 0..8usize { if registers & (1 << reg) != 0 { self.write_reg(reg, self.read32(address)); address = address.wrapping_add(4); } }
            if extra_lr_pc { self.write_reg(REG_PC, self.read32(address) & !1); }
            self.write_reg(13, self.read_reg(13).wrapping_add(count * 4));
        } else {
            let new_sp = self.read_reg(13).wrapping_sub(count * 4);
            let mut address = new_sp;
            for reg in 0..8usize { if registers & (1 << reg) != 0 { self.write32(address & !3, self.read_reg(reg)); address = address.wrapping_add(4); } }
            if extra_lr_pc { self.write32(address & !3, self.read_reg(REG_LR)); }
            self.write_reg(13, new_sp);
        }
    }

    fn execute_thumb_multiple(&mut self, load: bool, rb: usize, register_list: u8) {
        let mut address = self.read_reg(rb);
        for reg in 0..8usize { if register_list & (1 << reg) != 0 { if load { self.write_reg(reg, self.read32(address)); } else { self.write32(address & !3, self.read_reg(reg)); } address = address.wrapping_add(4); } }
        self.write_reg(rb, address);
    }

    fn execute_thumb_swi(&mut self, comment: u8) { self.halt_with_exception("SWI", u32::from(comment)); }
}

impl Runtime {
    pub fn halt_with_exception(&mut self, kind: &str, value: u32) -> ! {
        panic!("ARM7TDMI exception {kind} value {value:#x} at PC {:#010x}", self.read_reg(REG_PC));
    }

    pub fn shift_operand(&self, value: u32, kind: u8, amount: u8, by_register: bool, rs: usize) -> u32 {
        let shift = match kind { 0 => ShiftKind::Lsl, 1 => ShiftKind::Lsr, 2 => ShiftKind::Asr, _ => ShiftKind::Ror };
        let actual = if by_register { (self.read_reg(rs) & 0xff) as u8 } else { amount };
        self.shift(value, shift, actual, by_register).value
    }

    pub fn pc_value(&self, mode_thumb: bool) -> u32 { arm7tdmi::architectural_pc(self.read_reg(REG_PC), mode_thumb) }
}
