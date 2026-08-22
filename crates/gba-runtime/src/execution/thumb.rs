use super::{helpers::set_logic_flags, ExceptionKind, Runtime, REG_LR, REG_PC};
use crate::arm7tdmi::{self, ShiftKind};

impl Runtime {
    pub fn execute_thumb_instruction(&mut self, raw: u16) -> Option<(u32, bool)> {
        if raw & 0xff00 == 0xdf00 {
            return Some(self.raise_exception(ExceptionKind::SoftwareInterrupt));
        }
        if raw & 0xf800 == 0x0000 {
            let k = ((raw >> 11) & 3) as u8;
            let off = ((raw >> 6) & 31) as u8;
            let rd = (raw & 7) as usize;
            let rs = ((raw >> 3) & 7) as usize;
            let kind = match k {
                0 => ShiftKind::Lsl,
                1 => ShiftKind::Lsr,
                _ => ShiftKind::Asr,
            };
            let r = arm7tdmi::shift_immediate(self.read_reg(rs), kind, off, self.nzcv().c);
            self.write_reg(rd, r.value);
            self.set_flags(arm7tdmi::Nzcv::new(
                r.value & 0x8000_0000 != 0,
                r.value == 0,
                r.carry,
                self.nzcv().v,
            ));
            return None;
        }
        if raw & 0xf800 == 0x1800 {
            let sub = raw & (1 << 9) != 0;
            let imm = raw & (1 << 10) != 0;
            let rd = (raw & 7) as usize;
            let rs = ((raw >> 3) & 7) as usize;
            let rhs = if imm { ((raw >> 6) & 7) as u32 } else { self.read_reg(((raw >> 6) & 7) as usize) };
            if sub { self.sub(rd, self.read_reg(rs), rhs, true) } else { self.add(rd, self.read_reg(rs), rhs, true) }
            return None;
        }
        if raw & 0xf800 == 0x2000 {
            let rd = ((raw >> 8) & 7) as usize;
            let v = (raw & 255) as u32;
            self.write_reg(rd, v);
            self.set_flags(arm7tdmi::Nzcv::new(v & 0x8000_0000 != 0, v == 0, self.nzcv().c, self.nzcv().v));
            return None;
        }
        if raw & 0xf800 == 0x3000 || raw & 0xf800 == 0x3800 {
            let sub = raw & 0x0800 != 0;
            let rd = ((raw >> 8) & 7) as usize;
            let v = (raw & 255) as u32;
            if sub { self.sub(rd, self.read_reg(rd), v, true) } else { self.add(rd, self.read_reg(rd), v, true) }
            return None;
        }
        if raw & 0xfc00 == 0x4000 {
            let op = ((raw >> 6) & 15) as u8;
            let rd = (raw & 7) as usize;
            let rs = ((raw >> 3) & 7) as usize;
            let a = self.read_reg(rd);
            let b = self.read_reg(rs);
            match op {
                0 => { let v = a & b; self.write_reg(rd, v); set_logic_flags(self, v, None); }
                1 => { let v = a ^ b; self.write_reg(rd, v); set_logic_flags(self, v, None); }
                2 => {
                    let n = (b & 255) as u8;
                    let r = arm7tdmi::shift_register(a, ShiftKind::Lsl, n, self.nzcv().c);
                    self.write_reg(rd, r.value);
                    if n != 0 { set_logic_flags(self, r.value, Some(r.carry)) } else { set_logic_flags(self, r.value, None) }
                }
                3 => {
                    let n = (b & 255) as u8;
                    let r = arm7tdmi::shift_register(a, ShiftKind::Lsr, n, self.nzcv().c);
                    self.write_reg(rd, r.value);
                    if n != 0 { set_logic_flags(self, r.value, Some(r.carry)) } else { set_logic_flags(self, r.value, None) }
                }
                4 => {
                    let n = (b & 255) as u8;
                    let r = arm7tdmi::shift_register(a, ShiftKind::Asr, n, self.nzcv().c);
                    self.write_reg(rd, r.value);
                    if n != 0 { set_logic_flags(self, r.value, Some(r.carry)) } else { set_logic_flags(self, r.value, None) }
                }
                5 => self.adc(rd, a, b, true),
                6 => self.sbc(rd, a, b, true),
                7 => {
                    let n = (b & 255) as u8;
                    let r = arm7tdmi::shift_register(a, ShiftKind::Ror, n, self.nzcv().c);
                    self.write_reg(rd, r.value);
                    if n != 0 { set_logic_flags(self, r.value, Some(r.carry)) } else { set_logic_flags(self, r.value, None) }
                }
                8 => set_logic_flags(self, a & b, None),
                9 => { let (v, f) = arm7tdmi::sub_with_borrow(0, b, false); self.write_reg(rd, v); self.set_flags(f) }
                10 => self.compare(a, b),
                11 => { let (_, f) = arm7tdmi::add_with_carry(a, b, false); self.set_flags(f) }
                12 => { let v = a | b; self.write_reg(rd, v); set_logic_flags(self, v, None); }
                13 => { let v = a.wrapping_mul(b); self.write_reg(rd, v); set_logic_flags(self, v, None); }
                14 => { let v = a & !b; self.write_reg(rd, v); set_logic_flags(self, v, None); }
                15 => { let v = !b; self.write_reg(rd, v); set_logic_flags(self, v, None); }
                _ => unreachable!(),
            }
            return None;
        }
        if raw & 0xfc00 == 0x4400 {
            let op = ((raw >> 8) & 3) as u8;
            let rd = ((((raw >> 7) & 1) << 3) | (raw & 7)) as usize;
            let rs = ((((raw >> 6) & 1) << 3) | ((raw >> 3) & 7)) as usize;
            match op {
                0 => self.write_reg(rd, self.read_reg(rd).wrapping_add(self.read_reg(rs))),
                1 => self.compare(self.read_reg(rd), self.read_reg(rs)),
                2 => {
                    let v = self.read_reg(rs);
                    if rd == REG_PC {
                        let (target, thumb) = arm7tdmi::exchange_target(v);
                        self.set_thumb(thumb);
                        self.write_reg(REG_PC, target);
                        return Some((target, thumb));
                    }
                    self.write_reg(rd, v)
                }
                _ => {
                    let (target, thumb) = arm7tdmi::exchange_target(self.read_reg(rs));
                    self.set_thumb(thumb);
                    self.write_reg(REG_PC, target);
                    return Some((target, thumb));
                }
            }
            return None;
        }
        if raw & 0xf800 == 0x4800 {
            let rd = ((raw >> 8) & 7) as usize;
            let a = (self.read_reg(REG_PC) & !3).wrapping_add(u32::from(raw & 255) * 4);
            self.write_reg(rd, self.read32(a));
            return None;
        }
        if raw & 0xf000 == 0x5000 {
            let op = ((raw >> 9) & 7) as u8;
            let rd = (raw & 7) as usize;
            let rb = ((raw >> 3) & 7) as usize;
            let ro = ((raw >> 6) & 7) as usize;
            let a = self.read_reg(rb).wrapping_add(self.read_reg(ro));
            match op {
                0 => self.write32(a & !3, self.read_reg(rd)),
                1 => self.write8(a, self.read_reg(rd) as u8),
                2 => self.write16(a, self.read_reg(rd) as u16),
                3 => self.write_reg(rd, self.read32(a)),
                4 => self.write_reg(rd, self.read8(a) as u32),
                5 => { let v = self.read8(a) as u32; self.write_reg(rd, if v & 0x80 != 0 { v | 0xffff_ff00 } else { v }); }
                6 => { let v = self.read16(a) as u32; self.write_reg(rd, if v & 0x8000 != 0 { v | 0xffff_0000 } else { v }); }
                7 => self.write_reg(rd, self.read16(a) as u32),
                _ => unreachable!(),
            }
            return None;
        }
        if raw & 0xe000 == 0x6000 {
            let load = raw & (1 << 11) != 0;
            let byte = raw & (1 << 12) != 0;
            let rd = (raw & 7) as usize;
            let rb = ((raw >> 3) & 7) as usize;
            let off = ((raw >> 6) & 31) as u32 * if byte { 1 } else { 4 };
            let a = self.read_reg(rb).wrapping_add(off);
            if load { self.write_reg(rd, if byte { self.read8(a) as u32 } else { self.read32(a) }) } else if byte { self.write8(a, self.read_reg(rd) as u8) } else { self.write32(a & !3, self.read_reg(rd)) }
            return None;
        }
        if raw & 0xf000 == 0x8000 {
            let load = raw & (1 << 11) != 0;
            let rd = (raw & 7) as usize;
            let rb = ((raw >> 3) & 7) as usize;
            let a = self.read_reg(rb).wrapping_add(((raw >> 6) & 31) as u32 * 2);
            if load { self.write_reg(rd, self.read16(a) as u32) } else { self.write16(a, self.read_reg(rd) as u16) }
            return None;
        }
        if raw & 0xf000 == 0x9000 {
            let load = raw & (1 << 11) != 0;
            let rd = ((raw >> 8) & 7) as usize;
            let a = self.read_reg(13).wrapping_add(u32::from(raw & 255) * 4);
            if load { self.write_reg(rd, self.read32(a)) } else { self.write32(a & !3, self.read_reg(rd)) }
            return None;
        }
        if raw & 0xf000 == 0xa000 {
            let rd = ((raw >> 8) & 7) as usize;
            let base = if raw & (1 << 11) != 0 { self.read_reg(13) } else { self.read_reg(REG_PC) & !3 };
            self.write_reg(rd, base.wrapping_add(u32::from(raw & 255) * 4));
            return None;
        }
        if raw & 0xff80 == 0xb000 {
            let imm = u32::from(raw & 127) << 2;
            let sp = self.read_reg(13);
            self.write_reg(13, if raw & 0x80 != 0 { sp.wrapping_sub(imm) } else { sp.wrapping_add(imm) });
            return None;
        }
        if raw & 0xfe00 == 0xb400 || raw & 0xfe00 == 0xbc00 {
            let load = raw & (1 << 11) != 0;
            let extra = raw & (1 << 8) != 0;
            let regs = (raw & 255) as u8;
            if load {
                let mut a = self.read_reg(13);
                for r in 0..8usize {
                    if regs & (1 << r) != 0 { self.write_reg(r, self.read32(a)); a = a.wrapping_add(4); }
                }
                if extra {
                    let t = self.read32(a) & !1;
                    self.write_reg(13, a.wrapping_add(4));
                    self.write_reg(REG_PC, t);
                    self.set_thumb(true);
                    return Some((t, true));
                }
                self.write_reg(13, a)
            } else {
                let sp = self.read_reg(13).wrapping_sub((regs.count_ones() + u32::from(extra)) * 4);
                let mut a = sp;
                for r in 0..8usize { if regs & (1 << r) != 0 { self.write32(a & !3, self.read_reg(r)); a = a.wrapping_add(4); } }
                if extra { self.write32(a & !3, self.read_reg(REG_LR)); }
                self.write_reg(13, sp)
            }
            return None;
        }
        if raw & 0xf000 == 0xc000 {
            let load = raw & (1 << 11) != 0;
            let rb = ((raw >> 8) & 7) as usize;
            let regs = (raw & 255) as u8;
            let mut a = self.read_reg(rb);
            for r in 0..8usize {
                if regs & (1 << r) != 0 {
                    if load { self.write_reg(r, self.read32(a)) } else { self.write32(a & !3, self.read_reg(r)) }
                    a = a.wrapping_add(4);
                }
            }
            self.write_reg(rb, a);
            return None;
        }
        if raw & 0xf800 == 0xe000 { return None; }
        Some(self.raise_exception(ExceptionKind::Undefined))
    }
}
