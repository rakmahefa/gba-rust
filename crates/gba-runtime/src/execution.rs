use super::{Runtime, REG_LR, REG_PC};
use crate::arm7tdmi::{self, ShiftKind};

fn arm_condition(raw: u32) -> u8 { (raw >> 28) as u8 }

fn arm_operand2(rt: &Runtime, raw: u32) -> (u32, Option<bool>) {
    if raw & (1 << 25) != 0 {
        let imm = raw & 0xff;
        let rotate = ((raw >> 8) & 0xf) * 2;
        let value = imm.rotate_right(rotate);
        let carry = if rotate == 0 { None } else { Some(value & 0x8000_0000 != 0) };
        return (value, carry);
    }
    let rm = (raw & 0xf) as usize;
    let value = rt.read_reg(rm);
    let kind = match (raw >> 5) & 0x3 {
        0 => ShiftKind::Lsl,
        1 => ShiftKind::Lsr,
        2 => ShiftKind::Asr,
        _ => ShiftKind::Ror,
    };
    if raw & 0x10 == 0 {
        let amount = ((raw >> 7) & 0x1f) as u8;
        let result = arm7tdmi::shift_immediate(value, kind, amount, rt.nzcv().c);
        let carry = if amount == 0 && matches!(kind, ShiftKind::Lsl) { None } else { Some(result.carry) };
        (result.value, carry)
    } else {
        let amount = (rt.read_reg(((raw >> 8) & 0xf) as usize) & 0xff) as u8;
        let result = arm7tdmi::shift_register(value, kind, amount, rt.nzcv().c);
        let carry = if amount == 0 { None } else { Some(result.carry) };
        (result.value, carry)
    }
}

fn set_logic_flags(rt: &mut Runtime, value: u32, carry: Option<bool>) {
    let old = rt.nzcv();
    rt.set_flags(arm7tdmi::Nzcv::new(value & 0x8000_0000 != 0, value == 0, carry.unwrap_or(old.c), old.v));
}

fn arm_data_processing(rt: &mut Runtime, raw: u32) -> Option<(u32, bool)> {
    let opcode = ((raw >> 21) & 0xf) as u8;
    let set_flags = raw & (1 << 20) != 0;
    let rn = ((raw >> 16) & 0xf) as usize;
    let rd = ((raw >> 12) & 0xf) as usize;
    let lhs = rt.read_reg(rn);
    let (rhs, sh_carry) = arm_operand2(rt, raw);

    match opcode {
        0 => { let v = lhs & rhs; if rd == REG_PC { rt.write_reg(rd, v & !3); return Some((v & !3, false)); } rt.write_reg(rd, v); if set_flags { set_logic_flags(rt, v, sh_carry); } }
        1 => { let v = lhs ^ rhs; if rd == REG_PC { rt.write_reg(rd, v & !3); return Some((v & !3, false)); } rt.write_reg(rd, v); if set_flags { set_logic_flags(rt, v, sh_carry); } }
        2 => { let (v, f) = arm7tdmi::sub_with_borrow(lhs, rhs, false); if rd == REG_PC { rt.write_reg(rd, v & !3); return Some((v & !3, false)); } rt.write_reg(rd, v); if set_flags { rt.set_flags(f); } }
        3 => { let (v, f) = arm7tdmi::sub_with_borrow(rhs, lhs, false); if rd == REG_PC { rt.write_reg(rd, v & !3); return Some((v & !3, false)); } rt.write_reg(rd, v); if set_flags { rt.set_flags(f); } }
        4 => { let (v, f) = arm7tdmi::add_with_carry(lhs, rhs, false); if rd == REG_PC { rt.write_reg(rd, v & !3); return Some((v & !3, false)); } rt.write_reg(rd, v); if set_flags { rt.set_flags(f); } }
        5 => { let (v, f) = arm7tdmi::add_with_carry(lhs, rhs, rt.nzcv().c); if rd == REG_PC { rt.write_reg(rd, v & !3); return Some((v & !3, false)); } rt.write_reg(rd, v); if set_flags { rt.set_flags(f); } }
        6 => { let (v, f) = arm7tdmi::sub_with_borrow(lhs, rhs, !rt.nzcv().c); if rd == REG_PC { rt.write_reg(rd, v & !3); return Some((v & !3, false)); } rt.write_reg(rd, v); if set_flags { rt.set_flags(f); } }
        7 => { let (v, f) = arm7tdmi::sub_with_borrow(rhs, lhs, !rt.nzcv().c); if rd == REG_PC { rt.write_reg(rd, v & !3); return Some((v & !3, false)); } rt.write_reg(rd, v); if set_flags { rt.set_flags(f); } }
        8 => { set_logic_flags(rt, lhs & rhs, sh_carry); }
        9 => { set_logic_flags(rt, lhs ^ rhs, sh_carry); }
        10 => { let (_, f) = arm7tdmi::sub_with_borrow(lhs, rhs, false); rt.set_flags(f); }
        11 => { let (_, f) = arm7tdmi::add_with_carry(lhs, rhs, false); rt.set_flags(f); }
        12 => { let v = lhs | rhs; if rd == REG_PC { rt.write_reg(rd, v & !3); return Some((v & !3, false)); } rt.write_reg(rd, v); if set_flags { set_logic_flags(rt, v, sh_carry); } }
        13 => { if rd == REG_PC { let v = rhs & !3; rt.write_reg(rd, v); return Some((v, false)); } rt.write_reg(rd, rhs); if set_flags { set_logic_flags(rt, rhs, sh_carry); } }
        14 => { let v = lhs & !rhs; if rd == REG_PC { rt.write_reg(rd, v & !3); return Some((v & !3, false)); } rt.write_reg(rd, v); if set_flags { set_logic_flags(rt, v, sh_carry); } }
        15 => { let v = !rhs; if rd == REG_PC { let target = v & !3; rt.write_reg(rd, target); return Some((target, false)); } rt.write_reg(rd, v); if set_flags { set_logic_flags(rt, v, sh_carry); } }
        _ => unreachable!(),
    }
    None
}

fn transfer_address(base: u32, offset: u32, up: bool, pre: bool) -> u32 {
    let indexed = if up { base.wrapping_add(offset) } else { base.wrapping_sub(offset) };
    if pre { indexed } else { base }
}

fn arm_single_transfer(rt: &mut Runtime, raw: u32) -> Option<(u32, bool)> {
    let load = raw & (1 << 20) != 0;
    let byte = raw & (1 << 22) != 0;
    let pre = raw & (1 << 24) != 0;
    let up = raw & (1 << 23) != 0;
    let write_back = raw & (1 << 21) != 0 || !pre;
    let rn = ((raw >> 16) & 0xf) as usize;
    let rd = ((raw >> 12) & 0xf) as usize;
    let base = rt.read_reg(rn);
    let offset = if raw & (1 << 25) == 0 { raw & 0xfff } else { arm_operand2(rt, raw).0 };
    let address = transfer_address(base, offset, up, pre);
    if load {
        let value = if byte { rt.read8(address) as u32 } else { rt.read32(address) };
        if rd == REG_PC { let target = value & !3; rt.write_reg(REG_PC, target); if write_back { rt.write_reg(rn, if up { base.wrapping_add(offset) } else { base.wrapping_sub(offset) }); } return Some((target, false)); }
        rt.write_reg(rd, value);
    } else if byte { rt.write8(address, rt.read_reg(rd) as u8); } else { rt.write32(address & !3, rt.read_reg(rd)); }
    if write_back { rt.write_reg(rn, if up { base.wrapping_add(offset) } else { base.wrapping_sub(offset) }); }
    None
}

fn arm_halfword_transfer(rt: &mut Runtime, raw: u32) -> Option<(u32, bool)> {
    let load = raw & (1 << 20) != 0;
    let signed = raw & (1 << 6) != 0;
    let halfword = raw & (1 << 5) != 0;
    let pre = raw & (1 << 24) != 0;
    let up = raw & (1 << 23) != 0;
    let write_back = raw & (1 << 21) != 0 || !pre;
    let rn = ((raw >> 16) & 0xf) as usize;
    let rd = ((raw >> 12) & 0xf) as usize;
    let base = rt.read_reg(rn);
    let offset = if raw & (1 << 22) != 0 { ((raw >> 4) & 0xf0) | (raw & 0xf) } else { raw & 0xf };
    let address = transfer_address(base, offset, up, pre);
    if load {
        let value = if halfword { let v = rt.read16(address) as u32; if signed && v & 0x8000 != 0 { v | 0xffff_0000 } else { v } } else { let v = rt.read8(address) as u32; if signed && v & 0x80 != 0 { v | 0xffff_ff00 } else { v } };
        if rd == REG_PC { let target = value & !3; rt.write_reg(REG_PC, target); if write_back { rt.write_reg(rn, if up { base.wrapping_add(offset) } else { base.wrapping_sub(offset) }); } return Some((target, false)); }
        rt.write_reg(rd, value);
    } else { rt.write16(address, rt.read_reg(rd) as u16); }
    if write_back { rt.write_reg(rn, if up { base.wrapping_add(offset) } else { base.wrapping_sub(offset) }); }
    None
}

fn arm_block_transfer(rt: &mut Runtime, raw: u32) -> Option<(u32, bool)> {
    let load = raw & (1 << 20) != 0;
    let pre = raw & (1 << 24) != 0;
    let up = raw & (1 << 23) != 0;
    let write_back = raw & (1 << 21) != 0;
    let rn = ((raw >> 16) & 0xf) as usize;
    let list = (raw & 0xffff) as u16;
    if list == 0 { return None; }
    let base = rt.read_reg(rn);
    let count = list.count_ones();
    let start = if up { base.wrapping_add(if pre { 4 } else { 0 }) } else { base.wrapping_sub(if pre { count * 4 } else { count.saturating_sub(1) * 4 }) };
    let mut address = start;
    let mut pc_target = None;
    for reg in 0..16usize {
        if list & (1 << reg) == 0 { continue; }
        if load { let value = rt.read32(address); rt.write_reg(reg, value); if reg == REG_PC { pc_target = Some(value & !3); } } else { rt.write32(address & !3, rt.read_reg(reg)); }
        address = address.wrapping_add(4);
    }
    if write_back { rt.write_reg(rn, if up { base.wrapping_add(count * 4) } else { base.wrapping_sub(count * 4) }); }
    pc_target.map(|target| (target, false))
}

impl Runtime {
    pub fn execute_arm_instruction(&mut self, raw: u32) -> Option<(u32, bool)> {
        if raw & 0x0f00_0000 == 0x0f00_0000 { self.halt_with_exception("SWI", raw & 0x00ff_ffff); }
        if !arm7tdmi::condition_holds(self.cpu.cpsr, arm_condition(raw)) { return None; }
        if raw & 0x0fff_fff0 == 0x012f_ff10 || raw & 0x0fff_fff0 == 0x012f_ff30 { return None; }
        if raw & 0x0e00_0000 == 0x0a00_0000 { return None; }
        if raw & 0x0fc0_00f0 == 0x0000_0090 {
            let rd = ((raw >> 16) & 0xf) as usize; let rn = ((raw >> 12) & 0xf) as usize; let rs = ((raw >> 8) & 0xf) as usize; let rm = (raw & 0xf) as usize;
            let mut value = self.read_reg(rm).wrapping_mul(self.read_reg(rs)); if raw & (1 << 21) != 0 { value = value.wrapping_add(self.read_reg(rn)); }
            self.write_reg(rd, value); if raw & (1 << 20) != 0 { let old = self.nzcv(); self.set_flags(arm7tdmi::Nzcv::new(value & 0x8000_0000 != 0, value == 0, old.c, old.v)); }
            return None;
        }
        if raw & 0x0f80_00f0 == 0x0080_0090 {
            let rd_hi = ((raw >> 16) & 0xf) as usize; let rd_lo = ((raw >> 12) & 0xf) as usize; let rs = ((raw >> 8) & 0xf) as usize; let rm = (raw & 0xf) as usize;
            let signed = raw & (1 << 22) != 0;
            let mut result = if signed { (self.read_reg(rm) as i32 as i64).wrapping_mul(self.read_reg(rs) as i32 as i64) as u64 } else { (self.read_reg(rm) as u64).wrapping_mul(self.read_reg(rs) as u64) };
            if raw & (1 << 21) != 0 { result = result.wrapping_add((u64::from(self.read_reg(rd_hi)) << 32) | u64::from(self.read_reg(rd_lo))); }
            self.write_reg(rd_lo, result as u32); self.write_reg(rd_hi, (result >> 32) as u32);
            if raw & (1 << 20) != 0 { let old = self.nzcv(); self.set_flags(arm7tdmi::Nzcv::new(result >> 63 != 0, result == 0, old.c, old.v)); }
            return None;
        }
        if raw & 0x0fb0_0ff0 == 0x0100_0090 {
            let rn = ((raw >> 16) & 0xf) as usize; let rd = ((raw >> 12) & 0xf) as usize; let rm = (raw & 0xf) as usize; let address = self.read_reg(rn);
            if raw & (1 << 22) != 0 { let old = self.read8(address); self.write8(address, self.read_reg(rm) as u8); self.write_reg(rd, old as u32); } else { let old = self.read32(address); self.write32(address & !3, self.read_reg(rm)); self.write_reg(rd, old); }
            return None;
        }
        if raw & 0x0e00_0000 == 0x0800_0000 { return arm_block_transfer(self, raw); }
        if raw & 0x0c00_0000 == 0x0400_0000 { return arm_single_transfer(self, raw); }
        if raw & 0x0e00_0090 == 0x0000_0090 { return arm_halfword_transfer(self, raw); }
        if raw & 0x0fbf_0fff == 0x010f_0000 { let rd = ((raw >> 12) & 0xf) as usize; self.write_reg(rd, self.cpu.cpsr); return None; }
        if raw & 0x0dbf_f000 == 0x0129_f000 || raw & 0x0dbf_f000 == 0x0329_f000 {
            let spsr = raw & (1 << 22) != 0; let field_mask = ((raw >> 16) & 0xf) as u8;
            let value = if raw & (1 << 25) != 0 { let imm = raw & 0xff; let rotate = ((raw >> 8) & 0xf) * 2; imm.rotate_right(rotate) } else { self.read_reg((raw & 0xf) as usize) };
            if !spsr { let mut cpsr = self.cpu.cpsr; if field_mask & 1 != 0 { cpsr = (cpsr & !0xff) | (value & 0xff); } if field_mask & 2 != 0 { cpsr = (cpsr & !0xff00) | (value & 0xff00); } if field_mask & 4 != 0 { cpsr = (cpsr & !0xff0000) | (value & 0xff0000); } if field_mask & 8 != 0 { cpsr = (cpsr & !0xff00_0000) | (value & 0xff00_0000); } self.cpu.cpsr = cpsr; self.cpu.thumb = cpsr & (1 << 5) != 0; }
            return None;
        }
        if raw & 0x0c00_0000 == 0 { return arm_data_processing(self, raw); }
        self.halt_with_exception("Undefined", raw)
    }

    pub fn execute_thumb_instruction(&mut self, raw: u16) -> Option<(u32, bool)> {
        if raw & 0xf800 == 0x0000 {
            let kind = ((raw >> 11) & 0x3) as u8; let offset = ((raw >> 6) & 0x1f) as u8; let rd = (raw & 7) as usize; let rs = ((raw >> 3) & 7) as usize;
            let shift = match kind { 0 => ShiftKind::Lsl, 1 => ShiftKind::Lsr, _ => ShiftKind::Asr }; let result = arm7tdmi::shift_immediate(self.read_reg(rs), shift, offset, self.nzcv().c);
            self.write_reg(rd, result.value); self.set_flags(arm7tdmi::Nzcv::new(result.value & 0x8000_0000 != 0, result.value == 0, result.carry, self.nzcv().v)); return None;
        }
        if raw & 0xf800 == 0x1800 {
            let sub = raw & (1 << 9) != 0; let immediate = raw & (1 << 10) != 0; let rd = (raw & 7) as usize; let rs = ((raw >> 3) & 7) as usize; let rhs = if immediate { ((raw >> 6) & 7) as u32 } else { self.read_reg(((raw >> 6) & 7) as usize) };
            if sub { self.sub(rd, self.read_reg(rs), rhs, true); } else { self.add(rd, self.read_reg(rs), rhs, true); } return None;
        }
        if raw & 0xf800 == 0x2000 { let rd = ((raw >> 8) & 7) as usize; let value = (raw & 0xff) as u32; self.write_reg(rd, value); self.set_flags(arm7tdmi::Nzcv::new(value & 0x8000_0000 != 0, value == 0, self.nzcv().c, self.nzcv().v)); return None; }
        if raw & 0xf800 == 0x3000 || raw & 0xf800 == 0x3800 { let sub = raw & 0x0800 != 0; let rd = ((raw >> 8) & 7) as usize; let value = (raw & 0xff) as u32; if sub { self.sub(rd, self.read_reg(rd), value, true); } else { self.add(rd, self.read_reg(rd), value, true); } return None; }
        if raw & 0xfc00 == 0x4000 {
            let opcode = ((raw >> 6) & 0xf) as u8; let rd = (raw & 7) as usize; let rs = ((raw >> 3) & 7) as usize; let lhs = self.read_reg(rd); let rhs = self.read_reg(rs);
            match opcode {
                0 => { let v = lhs & rhs; self.write_reg(rd, v); set_logic_flags(self, v, None); }
                1 => { let v = lhs ^ rhs; self.write_reg(rd, v); set_logic_flags(self, v, None); }
                2 => { let amount = (rhs & 0xff) as u8; let r = arm7tdmi::shift_register(lhs, ShiftKind::Lsl, amount, self.nzcv().c); self.write_reg(rd, r.value); if amount != 0 { set_logic_flags(self, r.value, Some(r.carry)); } else { set_logic_flags(self, r.value, None); } }
                3 => { let amount = (rhs & 0xff) as u8; let r = arm7tdmi::shift_register(lhs, ShiftKind::Lsr, amount, self.nzcv().c); self.write_reg(rd, r.value); if amount != 0 { set_logic_flags(self, r.value, Some(r.carry)); } else { set_logic_flags(self, r.value, None); } }
                4 => { let amount = (rhs & 0xff) as u8; let r = arm7tdmi::shift_register(lhs, ShiftKind::Asr, amount, self.nzcv().c); self.write_reg(rd, r.value); if amount != 0 { set_logic_flags(self, r.value, Some(r.carry)); } else { set_logic_flags(self, r.value, None); } }
                5 => self.adc(rd, lhs, rhs, true),
                6 => self.sbc(rd, lhs, rhs, true),
                7 => { let amount = (rhs & 0xff) as u8; let r = arm7tdmi::shift_register(lhs, ShiftKind::Ror, amount, self.nzcv().c); self.write_reg(rd, r.value); if amount != 0 { set_logic_flags(self, r.value, Some(r.carry)); } else { set_logic_flags(self, r.value, None); } }
                8 => set_logic_flags(self, lhs & rhs, None),
                9 => { let (v, f) = arm7tdmi::sub_with_borrow(0, rhs, false); self.write_reg(rd, v); self.set_flags(f); }
                10 => self.compare(lhs, rhs),
                11 => { let (_, f) = arm7tdmi::add_with_carry(lhs, rhs, false); self.set_flags(f); }
                12 => { let v = lhs | rhs; self.write_reg(rd, v); set_logic_flags(self, v, None); }
                13 => { let v = lhs.wrapping_mul(rhs); self.write_reg(rd, v); set_logic_flags(self, v, None); }
                14 => { let v = lhs & !rhs; self.write_reg(rd, v); set_logic_flags(self, v, None); }
                15 => { let v = !rhs; self.write_reg(rd, v); set_logic_flags(self, v, None); }
                _ => unreachable!(),
            }
            return None;
        }
        if raw & 0xfc00 == 0x4400 {
            let op = ((raw >> 8) & 3) as u8; let rd = (((raw >> 7) & 1) << 3 | (raw & 7)) as usize; let rs = (((raw >> 6) & 1) << 3 | ((raw >> 3) & 7)) as usize;
            match op {
                0 => self.write_reg(rd, self.read_reg(rd).wrapping_add(self.read_reg(rs))),
                1 => self.compare(self.read_reg(rd), self.read_reg(rs)),
                2 => { let value = self.read_reg(rs); if rd == REG_PC { let target = value & !1; self.write_reg(REG_PC, target); return Some((target, true)); } self.write_reg(rd, value); }
                _ => { let (target, thumb) = arm7tdmi::exchange_target(self.read_reg(rs)); self.set_thumb(thumb); self.write_reg(REG_PC, target); return Some((target, thumb)); }
            }
            return None;
        }
        if raw & 0xf800 == 0x4800 { let rd = ((raw >> 8) & 7) as usize; let address = (self.read_reg(REG_PC) & !3).wrapping_add(u32::from(raw & 0xff) * 4); self.write_reg(rd, self.read32(address)); return None; }
        if raw & 0xf000 == 0x5000 {
            let opcode = ((raw >> 9) & 7) as u8; let rd = (raw & 7) as usize; let rb = ((raw >> 3) & 7) as usize; let ro = ((raw >> 6) & 7) as usize; let address = self.read_reg(rb).wrapping_add(self.read_reg(ro));
            match opcode { 0 => self.write32(address & !3, self.read_reg(rd)), 1 => self.write8(address, self.read_reg(rd) as u8), 2 => self.write16(address, self.read_reg(rd) as u16), 3 => self.write_reg(rd, self.read32(address)), 4 => self.write_reg(rd, self.read8(address) as u32), 5 => { let v = self.read8(address) as u32; self.write_reg(rd, if v & 0x80 != 0 { v | 0xffff_ff00 } else { v }); }, 6 => { let v = self.read16(address) as u32; self.write_reg(rd, if v & 0x8000 != 0 { v | 0xffff_0000 } else { v }); }, 7 => self.write_reg(rd, self.read16(address) as u32), _ => unreachable!() }
            return None;
        }
        if raw & 0xe000 == 0x6000 {
            let load = raw & (1 << 11) != 0; let byte = raw & (1 << 12) != 0; let rd = (raw & 7) as usize; let rb = ((raw >> 3) & 7) as usize; let offset = ((raw >> 6) & 0x1f) as u32 * if byte { 1 } else { 4 }; let address = self.read_reg(rb).wrapping_add(offset);
            if load { self.write_reg(rd, if byte { self.read8(address) as u32 } else { self.read32(address) }); } else if byte { self.write8(address, self.read_reg(rd) as u8); } else { self.write32(address & !3, self.read_reg(rd)); } return None;
        }
        if raw & 0xf000 == 0x8000 { let load = raw & (1 << 11) != 0; let rd = (raw & 7) as usize; let rb = ((raw >> 3) & 7) as usize; let offset = ((raw >> 6) & 0x1f) as u32 * 2; let address = self.read_reg(rb).wrapping_add(offset); if load { self.write_reg(rd, self.read16(address) as u32); } else { self.write16(address, self.read_reg(rd) as u16); } return None; }
        if raw & 0xf000 == 0x9000 { let load = raw & (1 << 11) != 0; let rd = ((raw >> 8) & 7) as usize; let address = self.read_reg(13).wrapping_add(u32::from(raw & 0xff) * 4); if load { self.write_reg(rd, self.read32(address)); } else { self.write32(address & !3, self.read_reg(rd)); } return None; }
        if raw & 0xf000 == 0xa000 { let rd = ((raw >> 8) & 7) as usize; let base = if raw & (1 << 11) != 0 { self.read_reg(13) } else { self.read_reg(REG_PC) & !3 }; self.write_reg(rd, base.wrapping_add(u32::from(raw & 0xff) * 4)); return None; }
        if raw & 0xff80 == 0xb000 { let imm = u32::from(raw & 0x7f) << 2; let sp = self.read_reg(13); self.write_reg(13, if raw & 0x80 != 0 { sp.wrapping_sub(imm) } else { sp.wrapping_add(imm) }); return None; }
        if raw & 0xfe00 == 0xb400 || raw & 0xfe00 == 0xbc00 {
            let load = raw & (1 << 11) != 0; let extra = raw & (1 << 8) != 0; let regs = (raw & 0xff) as u8;
            if load { let mut address = self.read_reg(13); for reg in 0..8usize { if regs & (1 << reg) != 0 { self.write_reg(reg, self.read32(address)); address = address.wrapping_add(4); } } if extra { let target = self.read32(address) & !1; self.write_reg(13, address.wrapping_add(4)); self.write_reg(REG_PC, target); return Some((target, true)); } self.write_reg(13, address); }
            else { let count = regs.count_ones() + u32::from(extra); let sp = self.read_reg(13).wrapping_sub(count * 4); let mut address = sp; for reg in 0..8usize { if regs & (1 << reg) != 0 { self.write32(address & !3, self.read_reg(reg)); address = address.wrapping_add(4); } } if extra { self.write32(address & !3, self.read_reg(REG_LR)); } self.write_reg(13, sp); }
            return None;
        }
        if raw & 0xf000 == 0xc000 {
            let load = raw & (1 << 11) != 0; let rb = ((raw >> 8) & 7) as usize; let regs = (raw & 0xff) as u8; let mut address = self.read_reg(rb);
            for reg in 0..8usize { if regs & (1 << reg) != 0 { if load { self.write_reg(reg, self.read32(address)); } else { self.write32(address & !3, self.read_reg(reg)); } address = address.wrapping_add(4); } }
            self.write_reg(rb, address); return None;
        }
        if raw & 0xff00 == 0xdf00 { self.halt_with_exception("SWI", u32::from(raw & 0xff)); }
        if raw & 0xf800 == 0xe000 { return None; }
        self.halt_with_exception("Undefined", u32::from(raw))
    }

    pub fn halt_with_exception(&mut self, kind: &str, value: u32) -> ! {
        panic!("ARM7TDMI exception {kind} value {value:#x} at PC {:#010x}", self.read_reg(REG_PC));
    }
}
