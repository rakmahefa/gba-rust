use super::{helpers::{arm_operand2, condition, set_logic_flags}, ExceptionKind, Runtime, REG_LR, REG_PC};
use crate::arm7tdmi::{self, ShiftKind};

fn arm_data_processing(rt: &mut Runtime, raw: u32) -> Option<(u32, bool)> {
    let op = ((raw >> 21) & 0xf) as u8;
    let s = raw & (1 << 20) != 0;
    let rn = ((raw >> 16) & 0xf) as usize;
    let rd = ((raw >> 12) & 0xf) as usize;
    let lhs = rt.read_reg(rn);
    let (rhs, shift_carry) = arm_operand2(rt, raw);
    let mut result = None;
    match op {
        0 => {
            let v = lhs & rhs;
            if rd == REG_PC {
                rt.write_reg(rd, v & !3);
                result = if s { rt.exception_return(v) } else { Some((v & !3, false)) };
            } else {
                rt.write_reg(rd, v);
                if s { set_logic_flags(rt, v, shift_carry); }
            }
        }
        1 => {
            let v = lhs ^ rhs;
            if rd == REG_PC {
                rt.write_reg(rd, v & !3);
                result = if s { rt.exception_return(v) } else { Some((v & !3, false)) };
            } else {
                rt.write_reg(rd, v);
                if s { set_logic_flags(rt, v, shift_carry); }
            }
        }
        2 => {
            let (v, f) = arm7tdmi::sub_with_borrow(lhs, rhs, false);
            if rd == REG_PC {
                rt.write_reg(rd, v & !3);
                result = if s { rt.exception_return(v) } else { Some((v & !3, false)) };
            } else {
                rt.write_reg(rd, v);
                if s { rt.set_flags(f); }
            }
        }
        3 => {
            let (v, f) = arm7tdmi::sub_with_borrow(rhs, lhs, false);
            if rd == REG_PC {
                rt.write_reg(rd, v & !3);
                result = if s { rt.exception_return(v) } else { Some((v & !3, false)) };
            } else {
                rt.write_reg(rd, v);
                if s { rt.set_flags(f); }
            }
        }
        4 => {
            let (v, f) = arm7tdmi::add_with_carry(lhs, rhs, false);
            if rd == REG_PC {
                rt.write_reg(rd, v & !3);
                result = Some((v & !3, false));
            } else {
                rt.write_reg(rd, v);
                if s { rt.set_flags(f); }
            }
        }
        5 => {
            let (v, f) = arm7tdmi::add_with_carry(lhs, rhs, rt.nzcv().c);
            if rd == REG_PC {
                rt.write_reg(rd, v & !3);
                result = Some((v & !3, false));
            } else {
                rt.write_reg(rd, v);
                if s { rt.set_flags(f); }
            }
        }
        6 => {
            let (v, f) = arm7tdmi::sub_with_borrow(lhs, rhs, !rt.nzcv().c);
            if rd == REG_PC {
                rt.write_reg(rd, v & !3);
                result = if s { rt.exception_return(v) } else { Some((v & !3, false)) };
            } else {
                rt.write_reg(rd, v);
                if s { rt.set_flags(f); }
            }
        }
        7 => {
            let (v, f) = arm7tdmi::sub_with_borrow(rhs, lhs, !rt.nzcv().c);
            if rd == REG_PC {
                rt.write_reg(rd, v & !3);
                result = if s { rt.exception_return(v) } else { Some((v & !3, false)) };
            } else {
                rt.write_reg(rd, v);
                if s { rt.set_flags(f); }
            }
        }
        8 => set_logic_flags(rt, lhs & rhs, shift_carry),
        9 => set_logic_flags(rt, lhs ^ rhs, shift_carry),
        10 => {
            let (_, f) = arm7tdmi::sub_with_borrow(lhs, rhs, false);
            rt.set_flags(f);
        }
        11 => {
            let (_, f) = arm7tdmi::add_with_carry(lhs, rhs, false);
            rt.set_flags(f);
        }
        12 => {
            let v = lhs | rhs;
            if rd == REG_PC {
                rt.write_reg(rd, v & !3);
                result = if s { rt.exception_return(v) } else { Some((v & !3, false)) };
            } else {
                rt.write_reg(rd, v);
                if s { set_logic_flags(rt, v, shift_carry); }
            }
        }
        13 => {
            if rd == REG_PC {
                rt.write_reg(rd, rhs & !3);
                result = if s { rt.exception_return(rhs) } else { Some((rhs & !3, false)) };
            } else {
                rt.write_reg(rd, rhs);
                if s { set_logic_flags(rt, rhs, shift_carry); }
            }
        }
        14 => {
            let v = lhs & !rhs;
            if rd == REG_PC {
                rt.write_reg(rd, v & !3);
                result = if s { rt.exception_return(v) } else { Some((v & !3, false)) };
            } else {
                rt.write_reg(rd, v);
                if s { set_logic_flags(rt, v, shift_carry); }
            }
        }
        15 => {
            let v = !rhs;
            if rd == REG_PC {
                rt.write_reg(rd, v & !3);
                result = if s { rt.exception_return(v) } else { Some((v & !3, false)) };
            } else {
                rt.write_reg(rd, v);
                if s { set_logic_flags(rt, v, shift_carry); }
            }
        }
        _ => unreachable!(),
    }
    result
}

fn arm_halfword(rt: &mut Runtime, raw: u32) -> Option<(u32, bool)> {
    let load = raw & (1 << 20) != 0;
    let signed = raw & (1 << 6) != 0;
    let half = raw & (1 << 5) != 0;
    let pre = raw & (1 << 24) != 0;
    let up = raw & (1 << 23) != 0;
    let wb = raw & (1 << 21) != 0 || !pre;
    let rn = ((raw >> 16) & 15) as usize;
    let rd = ((raw >> 12) & 15) as usize;
    let base = rt.read_reg(rn);
    let off = if raw & (1 << 22) != 0 { ((raw >> 4) & 0xf0) | (raw & 0xf) } else { raw & 0xf };
    let addr = if pre { if up { base.wrapping_add(off) } else { base.wrapping_sub(off) } } else { base };
    if load {
        let v = if half {
            let x = rt.read16(addr) as u32;
            if signed && x & 0x8000 != 0 { x | 0xffff_0000 } else { x }
        } else {
            let x = rt.read8(addr) as u32;
            if signed && x & 0x80 != 0 { x | 0xffff_ff00 } else { x }
        };
        if rd == REG_PC {
            let t = v & !3;
            rt.write_reg(REG_PC, t);
            if wb { rt.write_reg(rn, if up { base.wrapping_add(off) } else { base.wrapping_sub(off) }); }
            return Some((t, false));
        }
        rt.write_reg(rd, v);
    } else {
        rt.write16(addr, rt.read_reg(rd) as u16);
    }
    if wb { rt.write_reg(rn, if up { base.wrapping_add(off) } else { base.wrapping_sub(off) }); }
    None
}

fn arm_single(rt: &mut Runtime, raw: u32) -> Option<(u32, bool)> {
    let load = raw & (1 << 20) != 0;
    let byte = raw & (1 << 22) != 0;
    let pre = raw & (1 << 24) != 0;
    let up = raw & (1 << 23) != 0;
    let wb = raw & (1 << 21) != 0 || !pre;
    let rn = ((raw >> 16) & 15) as usize;
    let rd = ((raw >> 12) & 15) as usize;
    let base = rt.read_reg(rn);
    let off = if raw & (1 << 25) == 0 { raw & 0xfff } else { arm_operand2(rt, raw).0 };
    let addr = if pre { if up { base.wrapping_add(off) } else { base.wrapping_sub(off) } } else { base };
    if load {
        let v = if byte { rt.read8(addr) as u32 } else { rt.read32(addr) };
        if rd == REG_PC {
            let t = v & !3;
            rt.write_reg(REG_PC, t);
            if wb { rt.write_reg(rn, if up { base.wrapping_add(off) } else { base.wrapping_sub(off) }); }
            return Some((t, false));
        }
        rt.write_reg(rd, v);
    } else if byte {
        rt.write8(addr, rt.read_reg(rd) as u8)
    } else {
        rt.write32(addr & !3, rt.read_reg(rd));
    }
    if wb { rt.write_reg(rn, if up { base.wrapping_add(off) } else { base.wrapping_sub(off) }); }
    None
}

fn arm_block(rt: &mut Runtime, raw: u32) -> Option<(u32, bool)> {
    let load = raw & (1 << 20) != 0;
    let pre = raw & (1 << 24) != 0;
    let up = raw & (1 << 23) != 0;
    let wb = raw & (1 << 21) != 0;
    let rn = ((raw >> 16) & 15) as usize;
    let list = (raw & 0xffff) as u16;
    if list == 0 { return None; }
    let base = rt.read_reg(rn);
    let count = list.count_ones();
    let mut addr = if up { base.wrapping_add(if pre { 4 } else { 0 }) } else { base.wrapping_sub(if pre { count * 4 } else { count.saturating_sub(1) * 4 }) };
    let mut pc = None;
    for r in 0..16usize {
        if list & (1 << r) == 0 { continue; }
        if load {
            let value = rt.read32(addr);
            rt.write_reg(r, value);
            if r == REG_PC { pc = Some(value & !3); }
        } else {
            let value = rt.read_reg(r);
            rt.write32(addr & !3, value);
        }
        addr = addr.wrapping_add(4);
    }
    if wb { rt.write_reg(rn, if up { base.wrapping_add(count * 4) } else { base.wrapping_sub(count * 4) }); }
    pc.map(|t| (t, false))
}

impl Runtime {
    pub fn execute_arm_instruction(&mut self, raw: u32) -> Option<(u32, bool)> {
        if raw & 0x0f00_0000 == 0x0f00_0000 { return Some(self.raise_exception(ExceptionKind::SoftwareInterrupt)); }
        if !arm7tdmi::condition_holds(self.cpu.cpsr, condition(raw)) { return None; }
        if raw & 0x0fff_fff0 == 0x012f_ff10 || raw & 0x0fff_fff0 == 0x012f_ff30 {
            let target = self.read_reg((raw & 15) as usize);
            return Some(self.exchange_target_for_dispatch(target));
        }
        if raw & 0x0e00_0000 == 0x0a00_0000 {
            let base = self.read_reg(REG_PC);
            let imm = ((raw & 0x00ff_ffff) << 2) as i32;
            let target = base.wrapping_add(imm as u32) & !3;
            if raw & (1 << 24) != 0 { self.write_reg(REG_LR, base.wrapping_sub(4)); }
            self.write_reg(REG_PC, target);
            return Some((target, false));
        }
        if raw & 0x0f80_00f0 == 0x0080_0090 {
            let hi = ((raw >> 16) & 15) as usize;
            let lo = ((raw >> 12) & 15) as usize;
            let rs = ((raw >> 8) & 15) as usize;
            let rm = (raw & 15) as usize;
            let signed = raw & (1 << 22) != 0;
            let mut x = if signed { (self.read_reg(rm) as i32 as i64).wrapping_mul(self.read_reg(rs) as i32 as i64) as u64 } else { (self.read_reg(rm) as u64).wrapping_mul(self.read_reg(rs) as u64) };
            if raw & (1 << 21) != 0 { x = x.wrapping_add((u64::from(self.read_reg(hi)) << 32) | u64::from(self.read_reg(lo))); }
            self.write_reg(lo, x as u32);
            self.write_reg(hi, (x >> 32) as u32);
            if raw & (1 << 20) != 0 { let o = self.nzcv(); self.set_flags(arm7tdmi::Nzcv::new(x >> 63 != 0, x == 0, o.c, o.v)); }
            return None;
        }
        if raw & 0x0fc0_00f0 == 0x0000_0090 {
            let rd = ((raw >> 16) & 15) as usize;
            let rn = ((raw >> 12) & 15) as usize;
            let rs = ((raw >> 8) & 15) as usize;
            let rm = (raw & 15) as usize;
            let mut x = self.read_reg(rm).wrapping_mul(self.read_reg(rs));
            if raw & (1 << 21) != 0 { x = x.wrapping_add(self.read_reg(rn)); }
            self.write_reg(rd, x);
            if raw & (1 << 20) != 0 { let o = self.nzcv(); self.set_flags(arm7tdmi::Nzcv::new(x & 0x8000_0000 != 0, x == 0, o.c, o.v)); }
            return None;
        }
        if raw & 0x0fb0_0ff0 == 0x0100_0090 {
            let rn = ((raw >> 16) & 15) as usize;
            let rd = ((raw >> 12) & 15) as usize;
            let rm = (raw & 15) as usize;
            let a = self.read_reg(rn);
            if raw & (1 << 22) != 0 {
                let old = self.read8(a);
                self.write8(a, self.read_reg(rm) as u8);
                self.write_reg(rd, old as u32);
            } else {
                let old = self.read32(a);
                self.write32(a & !3, self.read_reg(rm));
                self.write_reg(rd, old);
            }
            return None;
        }
        if raw & 0x0e00_0090 == 0x0000_0090 { return arm_halfword(self, raw); }
        if raw & 0x0e00_0000 == 0x0800_0000 { return arm_block(self, raw); }
        if raw & 0x0c00_0000 == 0x0400_0000 { return arm_single(self, raw); }
        if raw & 0x0fbf_0fff == 0x010f_0000 {
            let rd = ((raw >> 12) & 15) as usize;
            self.write_reg(rd, self.cpu.cpsr);
            return None;
        }
        if raw & 0x0db0_f000 == 0x0120_f000 {
            let spsr = raw & (1 << 22) != 0;
            let mask = ((raw >> 16) & 15) as u8;
            let value = if raw & (1 << 25) != 0 {
                let imm = raw & 255;
                let rot = ((raw >> 8) & 15) * 2;
                imm.rotate_right(rot)
            } else {
                self.read_reg((raw & 15) as usize)
            };
            if spsr {
                self.cpu.set_spsr(value);
            } else {
                let mode = self.mode();
                if mode.privileged() {
                    let mut c = self.cpu.cpsr;
                    if mask & 1 != 0 { c = (c & !0xff) | (value & 0xff); }
                    if mask & 2 != 0 { c = (c & !0xff00) | (value & 0xff00); }
                    if mask & 4 != 0 { c = (c & !0xff0000) | (value & 0xff0000); }
                    if mask & 8 != 0 { c = (c & !0xff00_0000) | (value & 0xff00_0000); }
                    let nm = crate::CpuMode::from_cpsr(c).unwrap_or(mode);
                    if nm != mode { self.cpu.switch_mode(nm); }
                    self.cpu.cpsr = c;
                    self.cpu.thumb = c & (1 << 5) != 0;
                } else {
                    self.cpu.cpsr = (self.cpu.cpsr & !0xff) | (value & 0xff);
                }
            }
            return None;
        }
        if raw & 0x0c00_0000 == 0 { return arm_data_processing(self, raw); }
        Some(self.raise_exception(ExceptionKind::Undefined))
    }
}
