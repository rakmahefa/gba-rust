use super::{ExceptionKind, Runtime, REG_LR, REG_PC};
use crate::arm7tdmi::{self, ShiftKind};

fn condition(raw: u32) -> u8 { (raw >> 28) as u8 }

fn arm_operand2(rt: &Runtime, raw: u32) -> (u32, Option<bool>) {
    if raw & (1 << 25) != 0 {
        let imm = raw & 0xff;
        let rotate = ((raw >> 8) & 0xf) * 2;
        let value = imm.rotate_right(rotate);
        return (value, if rotate == 0 { None } else { Some(value & 0x8000_0000 != 0) });
    }
    let value = rt.read_reg((raw & 0xf) as usize);
    let kind = match (raw >> 5) & 3 { 0 => ShiftKind::Lsl, 1 => ShiftKind::Lsr, 2 => ShiftKind::Asr, _ => ShiftKind::Ror };
    let amount = if raw & 0x10 == 0 { ((raw >> 7) & 0x1f) as u8 } else { (rt.read_reg(((raw >> 8) & 0xf) as usize) & 0xff) as u8 };
    let result = if raw & 0x10 == 0 { arm7tdmi::shift_immediate(value, kind, amount, rt.nzcv().c) } else { arm7tdmi::shift_register(value, kind, amount, rt.nzcv().c) };
    let carry = if amount == 0 && raw & 0x10 != 0 { None } else if amount == 0 && matches!(kind, ShiftKind::Lsl) { None } else { Some(result.carry) };
    (result.value, carry)
}

fn set_logic_flags(rt: &mut Runtime, value: u32, carry: Option<bool>) {
    let old = rt.nzcv();
    rt.set_flags(arm7tdmi::Nzcv::new(value & 0x8000_0000 != 0, value == 0, carry.unwrap_or(old.c), old.v));
}

fn arm_data_processing(rt: &mut Runtime, raw: u32) -> Option<(u32, bool)> {
    let op = ((raw >> 21) & 0xf) as u8;
    let s = raw & (1 << 20) != 0;
    let rn = ((raw >> 16) & 0xf) as usize;
    let rd = ((raw >> 12) & 0xf) as usize;
    let lhs = rt.read_reg(rn);
    let (rhs, shift_carry) = arm_operand2(rt, raw);
    let mut result = None;
    match op {
        0 => { let v = lhs & rhs; if rd == REG_PC { rt.write_reg(rd, v & !3); result = if s { rt.exception_return(v) } else { Some((v & !3, false)) }; } else { rt.write_reg(rd, v); if s { set_logic_flags(rt, v, shift_carry); } } }
        1 => { let v = lhs ^ rhs; if rd == REG_PC { rt.write_reg(rd, v & !3); result = if s { rt.exception_return(v) } else { Some((v & !3, false)) }; } else { rt.write_reg(rd, v); if s { set_logic_flags(rt, v, shift_carry); } } }
        2 => { let (v,f)=arm7tdmi::sub_with_borrow(lhs,rhs,false); if rd==REG_PC { rt.write_reg(rd,v&!3); result=if s { rt.exception_return(v) } else { Some((v&!3,false)) }; } else { rt.write_reg(rd,v); if s { rt.set_flags(f); } } }
        3 => { let (v,f)=arm7tdmi::sub_with_borrow(rhs,lhs,false); if rd==REG_PC { rt.write_reg(rd,v&!3); result=if s { rt.exception_return(v) } else { Some((v&!3,false)) }; } else { rt.write_reg(rd,v); if s { rt.set_flags(f); } } }
        4 => { let (v,f)=arm7tdmi::add_with_carry(lhs,rhs,false); if rd==REG_PC { rt.write_reg(rd,v&!3); result=Some((v&!3,false)); } else { rt.write_reg(rd,v); if s { rt.set_flags(f); } } }
        5 => { let (v,f)=arm7tdmi::add_with_carry(lhs,rhs,rt.nzcv().c); if rd==REG_PC { rt.write_reg(rd,v&!3); result=Some((v&!3,false)); } else { rt.write_reg(rd,v); if s { rt.set_flags(f); } } }
        6 => { let (v,f)=arm7tdmi::sub_with_borrow(lhs,rhs,!rt.nzcv().c); if rd==REG_PC { rt.write_reg(rd,v&!3); result=if s { rt.exception_return(v) } else { Some((v&!3,false)) }; } else { rt.write_reg(rd,v); if s { rt.set_flags(f); } } }
        7 => { let (v,f)=arm7tdmi::sub_with_borrow(rhs,lhs,!rt.nzcv().c); if rd==REG_PC { rt.write_reg(rd,v&!3); result=if s { rt.exception_return(v) } else { Some((v&!3,false)) }; } else { rt.write_reg(rd,v); if s { rt.set_flags(f); } } }
        8 => set_logic_flags(rt,lhs&rhs,shift_carry),
        9 => set_logic_flags(rt,lhs^rhs,shift_carry),
        10 => { let (_,f)=arm7tdmi::sub_with_borrow(lhs,rhs,false); rt.set_flags(f); }
        11 => { let (_,f)=arm7tdmi::add_with_carry(lhs,rhs,false); rt.set_flags(f); }
        12 => { let v=lhs|rhs; if rd==REG_PC { rt.write_reg(rd,v&!3); result=if s { rt.exception_return(v) } else { Some((v&!3,false)) }; } else { rt.write_reg(rd,v); if s { set_logic_flags(rt,v,shift_carry); } } }
        13 => { if rd==REG_PC { rt.write_reg(rd,rhs&!3); result=if s { rt.exception_return(rhs) } else { Some((rhs&!3,false)) }; } else { rt.write_reg(rd,rhs); if s { set_logic_flags(rt,rhs,shift_carry); } } }
        14 => { let v=lhs&!rhs; if rd==REG_PC { rt.write_reg(rd,v&!3); result=if s { rt.exception_return(v) } else { Some((v&!3,false)) }; } else { rt.write_reg(rd,v); if s { set_logic_flags(rt,v,shift_carry); } } }
        15 => { let v=!rhs; if rd==REG_PC { rt.write_reg(rd,v&!3); result=if s { rt.exception_return(v) } else { Some((v&!3,false)) }; } else { rt.write_reg(rd,v); if s { set_logic_flags(rt,v,shift_carry); } } }
        _ => unreachable!(),
    }
    result
}

fn arm_halfword(rt:&mut Runtime,raw:u32)->Option<(u32,bool)> {
    let load=raw&(1<<20)!=0; let signed=raw&(1<<6)!=0; let half=raw&(1<<5)!=0; let pre=raw&(1<<24)!=0; let up=raw&(1<<23)!=0; let wb=raw&(1<<21)!=0||!pre;
    let rn=((raw>>16)&15) as usize; let rd=((raw>>12)&15) as usize; let base=rt.read_reg(rn);
    let off=if raw&(1<<22)!=0 { ((raw>>4)&0xf0)|(raw&0xf) } else { raw&0xf };
    let addr=if pre { if up {base.wrapping_add(off)} else {base.wrapping_sub(off)} } else {base};
    if load {
        let v=if half { let x=rt.read16(addr) as u32; if signed&&x&0x8000!=0{x|0xffff_0000}else{x} } else { let x=rt.read8(addr) as u32; if signed&&x&0x80!=0{x|0xffff_ff00}else{x} };
        if rd==REG_PC { let t=v&!3; rt.write_reg(REG_PC,t); if wb { rt.write_reg(rn,if up{base.wrapping_add(off)}else{base.wrapping_sub(off)}); } return Some((t,false)); }
        rt.write_reg(rd,v);
    } else { rt.write16(addr,rt.read_reg(rd) as u16); }
    if wb { rt.write_reg(rn,if up{base.wrapping_add(off)}else{base.wrapping_sub(off)}); }
    None
}

fn arm_single(rt:&mut Runtime,raw:u32)->Option<(u32,bool)> {
    let load=raw&(1<<20)!=0; let byte=raw&(1<<22)!=0; let pre=raw&(1<<24)!=0; let up=raw&(1<<23)!=0; let wb=raw&(1<<21)!=0||!pre;
    let rn=((raw>>16)&15) as usize; let rd=((raw>>12)&15) as usize; let base=rt.read_reg(rn); let off=if raw&(1<<25)==0{raw&0xfff}else{arm_operand2(rt,raw).0};
    let addr=if pre{if up{base.wrapping_add(off)}else{base.wrapping_sub(off)}}else{base};
    if load { let v=if byte{rt.read8(addr) as u32}else{rt.read32(addr)}; if rd==REG_PC{let t=v&!3;rt.write_reg(REG_PC,t);if wb{rt.write_reg(rn,if up{base.wrapping_add(off)}else{base.wrapping_sub(off)});}return Some((t,false));}rt.write_reg(rd,v); }
    else if byte{rt.write8(addr,rt.read_reg(rd) as u8)}else{rt.write32(addr&!3,rt.read_reg(rd));}
    if wb{rt.write_reg(rn,if up{base.wrapping_add(off)}else{base.wrapping_sub(off)});}None
}

fn arm_block(rt:&mut Runtime,raw:u32)->Option<(u32,bool)> {
    let load=raw&(1<<20)!=0;let pre=raw&(1<<24)!=0;let up=raw&(1<<23)!=0;let wb=raw&(1<<21)!=0;let rn=((raw>>16)&15) as usize;let list=(raw&0xffff) as u16;if list==0{return None;}
    let base=rt.read_reg(rn);let count=list.count_ones();let mut addr=if up{base.wrapping_add(if pre{4}else{0})}else{base.wrapping_sub(if pre{count*4}else{count.saturating_sub(1)*4})};let mut pc=None;
    for r in 0..16usize{if list&(1<<r)==0{continue;}if load{let v=rt.read32(addr);rt.write_reg(r,v);if r==REG_PC{pc=Some(v&!3);}}else{rt.write32(addr&!3,rt.read_reg(r));}addr=addr.wrapping_add(4);}
    if wb{rt.write_reg(rn,if up{base.wrapping_add(count*4)}else{base.wrapping_sub(count*4)});}pc.map(|t|(t,false))
}

impl Runtime {
    pub fn execute_arm_instruction(&mut self, raw:u32)->Option<(u32,bool)> {
        if raw&0x0f00_0000==0x0f00_0000{return Some(self.raise_exception(ExceptionKind::SoftwareInterrupt));}
        if !arm7tdmi::condition_holds(self.cpu.cpsr,condition(raw)){return None;}
        if raw&0x0fff_fff0==0x012f_ff10||raw&0x0fff_fff0==0x012f_ff30{return Some(self.dispatch_exchange(self.read_reg((raw&15) as usize)));}
        if raw&0x0e00_0000==0x0a00_0000{let base=self.read_reg(REG_PC);let imm=((raw&0x00ff_ffff)<<2) as i32;let target=base.wrapping_add(imm as u32)&!3;if raw&(1<<24)!=0{self.write_reg(REG_LR,base.wrapping_sub(4));}self.write_reg(REG_PC,target);return Some((target,false));}
        if raw&0x0f80_00f0==0x0080_0090{let hi=((raw>>16)&15) as usize;let lo=((raw>>12)&15) as usize;let rs=((raw>>8)&15) as usize;let rm=(raw&15) as usize;let signed=raw&(1<<22)!=0;let mut x=if signed{(self.read_reg(rm) as i32 as i64).wrapping_mul(self.read_reg(rs) as i32 as i64) as u64}else{(self.read_reg(rm) as u64).wrapping_mul(self.read_reg(rs) as u64)};if raw&(1<<21)!=0{x=x.wrapping_add((u64::from(self.read_reg(hi))<<32)|u64::from(self.read_reg(lo)));}self.write_reg(lo,x as u32);self.write_reg(hi,(x>>32) as u32);if raw&(1<<20)!=0{let o=self.nzcv();self.set_flags(arm7tdmi::Nzcv::new(x>>63!=0,x==0,o.c,o.v));}return None;}
        if raw&0x0fc0_00f0==0x0000_0090{let rd=((raw>>16)&15) as usize;let rn=((raw>>12)&15) as usize;let rs=((raw>>8)&15) as usize;let rm=(raw&15) as usize;let mut x=self.read_reg(rm).wrapping_mul(self.read_reg(rs));if raw&(1<<21)!=0{x=x.wrapping_add(self.read_reg(rn));}self.write_reg(rd,x);if raw&(1<<20)!=0{let o=self.nzcv();self.set_flags(arm7tdmi::Nzcv::new(x&0x8000_0000!=0,x==0,o.c,o.v));}return None;}
        if raw&0x0fb0_0ff0==0x0100_0090{let rn=((raw>>16)&15) as usize;let rd=((raw>>12)&15) as usize;let rm=(raw&15) as usize;let a=self.read_reg(rn);if raw&(1<<22)!=0{let old=self.read8(a);self.write8(a,self.read_reg(rm) as u8);self.write_reg(rd,old as u32);}else{let old=self.read32(a);self.write32(a&!3,self.read_reg(rm));self.write_reg(rd,old);}return None;}
        if raw&0x0e00_0090==0x0000_0090{return arm_halfword(self,raw);}
        if raw&0x0e00_0000==0x0800_0000{return arm_block(self,raw);}
        if raw&0x0c00_0000==0x0400_0000{return arm_single(self,raw);}
        if raw&0x0fbf_0fff==0x010f_0000{let rd=((raw>>12)&15) as usize;self.write_reg(rd,self.cpu.cpsr);return None;}
        if raw&0x0db0_f000==0x0120_f000{let spsr=raw&(1<<22)!=0;let mask=((raw>>16)&15) as u8;let value=if raw&(1<<25)!=0{let imm=raw&255;let rot=((raw>>8)&15)*2;imm.rotate_right(rot)}else{self.read_reg((raw&15) as usize)};if spsr{self.cpu.set_spsr(value);}else{let mode=self.mode();if mode.privileged(){let mut c=self.cpu.cpsr;if mask&1!=0{c=(c&!0xff)|(value&0xff);}if mask&2!=0{c=(c&!0xff00)|(value&0xff00);}if mask&4!=0{c=(c&!0xff0000)|(value&0xff0000);}if mask&8!=0{c=(c&!0xff00_0000)|(value&0xff00_0000);}let nm=crate::CpuMode::from_cpsr(c).unwrap_or(mode);if nm!=mode{self.cpu.switch_mode(nm);}self.cpu.cpsr=c;self.cpu.thumb=c&(1<<5)!=0;}else{self.cpu.cpsr=(self.cpu.cpsr&!0xff)|(value&0xff);}}return None;}
        if raw&0x0c00_0000==0{return arm_data_processing(self,raw);}
        Some(self.raise_exception(ExceptionKind::Undefined))
    }

    pub fn execute_thumb_instruction(&mut self, raw:u16)->Option<(u32,bool)> {
        if raw&0xff00==0xdf00{return Some(self.raise_exception(ExceptionKind::SoftwareInterrupt));}
        if raw&0xf800==0x0000{let k=((raw>>11)&3) as u8;let off=((raw>>6)&31) as u8;let rd=(raw&7) as usize;let rs=((raw>>3)&7) as usize;let kind=match k{0=>ShiftKind::Lsl,1=>ShiftKind::Lsr,_=>ShiftKind::Asr};let r=arm7tdmi::shift_immediate(self.read_reg(rs),kind,off,self.nzcv().c);self.write_reg(rd,r.value);self.set_flags(arm7tdmi::Nzcv::new(r.value&0x8000_0000!=0,r.value==0,r.carry,self.nzcv().v));return None;}
        if raw&0xf800==0x1800{let sub=raw&(1<<9)!=0;let imm=raw&(1<<10)!=0;let rd=(raw&7) as usize;let rs=((raw>>3)&7) as usize;let rhs=if imm{((raw>>6)&7) as u32}else{self.read_reg(((raw>>6)&7) as usize)};if sub{self.sub(rd,self.read_reg(rs),rhs,true)}else{self.add(rd,self.read_reg(rs),rhs,true)}return None;}
        if raw&0xf800==0x2000{let rd=((raw>>8)&7) as usize;let v=(raw&255) as u32;self.write_reg(rd,v);self.set_flags(arm7tdmi::Nzcv::new(v&0x8000_0000!=0,v==0,self.nzcv().c,self.nzcv().v));return None;}
        if raw&0xf800==0x3000||raw&0xf800==0x3800{let sub=raw&0x0800!=0;let rd=((raw>>8)&7) as usize;let v=(raw&255) as u32;if sub{self.sub(rd,self.read_reg(rd),v,true)}else{self.add(rd,self.read_reg(rd),v,true)}return None;}
        if raw&0xfc00==0x4000{let op=((raw>>6)&15) as u8;let rd=(raw&7) as usize;let rs=((raw>>3)&7) as usize;let a=self.read_reg(rd);let b=self.read_reg(rs);match op{0=>{let v=a&b;self.write_reg(rd,v);set_logic_flags(self,v,None)},1=>{let v=a^b;self.write_reg(rd,v);set_logic_flags(self,v,None)},2=>{let n=(b&255) as u8;let r=arm7tdmi::shift_register(a,ShiftKind::Lsl,n,self.nzcv().c);self.write_reg(rd,r.value);if n!=0{set_logic_flags(self,r.value,Some(r.carry))}else{set_logic_flags(self,r.value,None)}},3=>{let n=(b&255) as u8;let r=arm7tdmi::shift_register(a,ShiftKind::Lsr,n,self.nzcv().c);self.write_reg(rd,r.value);if n!=0{set_logic_flags(self,r.value,Some(r.carry))}else{set_logic_flags(self,r.value,None)}},4=>{let n=(b&255) as u8;let r=arm7tdmi::shift_register(a,ShiftKind::Asr,n,self.nzcv().c);self.write_reg(rd,r.value);if n!=0{set_logic_flags(self,r.value,Some(r.carry))}else{set_logic_flags(self,r.value,None)}},5=>self.adc(rd,a,b,true),6=>self.sbc(rd,a,b,true),7=>{let n=(b&255) as u8;let r=arm7tdmi::shift_register(a,ShiftKind::Ror,n,self.nzcv().c);self.write_reg(rd,r.value);if n!=0{set_logic_flags(self,r.value,Some(r.carry))}else{set_logic_flags(self,r.value,None)}},8=>set_logic_flags(self,a&b,None),9=>{let(v,f)=arm7tdmi::sub_with_borrow(0,b,false);self.write_reg(rd,v);self.set_flags(f)},10=>self.compare(a,b),11=>{let(_,f)=arm7tdmi::add_with_carry(a,b,false);self.set_flags(f)},12=>{let v=a|b;self.write_reg(rd,v);set_logic_flags(self,v,None)},13=>{let v=a.wrapping_mul(b);self.write_reg(rd,v);set_logic_flags(self,v,None)},14=>{let v=a&!b;self.write_reg(rd,v);set_logic_flags(self,v,None)},15=>{let v=!b;self.write_reg(rd,v);set_logic_flags(self,v,None)},_=>unreachable!()}return None;}
        if raw&0xfc00==0x4400{let op=((raw>>8)&3) as u8;let rd=((((raw>>7)&1)<<3)|(raw&7)) as usize;let rs=((((raw>>6)&1)<<3)|((raw>>3)&7)) as usize;match op{0=>self.write_reg(rd,self.read_reg(rd).wrapping_add(self.read_reg(rs))),1=>self.compare(self.read_reg(rd),self.read_reg(rs)),2=>{let v=self.read_reg(rs);if rd==REG_PC{let(target,thumb)=arm7tdmi::exchange_target(v);self.set_thumb(thumb);self.write_reg(REG_PC,target);return Some((target,thumb));}self.write_reg(rd,v)},_=>{let(target,thumb)=arm7tdmi::exchange_target(self.read_reg(rs));self.set_thumb(thumb);self.write_reg(REG_PC,target);return Some((target,thumb))}}return None;}
        if raw&0xf800==0x4800{let rd=((raw>>8)&7) as usize;let a=(self.read_reg(REG_PC)&!3).wrapping_add(u32::from(raw&255)*4);self.write_reg(rd,self.read32(a));return None;}
        if raw&0xf000==0x5000{let op=((raw>>9)&7) as u8;let rd=(raw&7) as usize;let rb=((raw>>3)&7) as usize;let ro=((raw>>6)&7) as usize;let a=self.read_reg(rb).wrapping_add(self.read_reg(ro));match op{0=>self.write32(a&!3,self.read_reg(rd)),1=>self.write8(a,self.read_reg(rd) as u8),2=>self.write16(a,self.read_reg(rd) as u16),3=>self.write_reg(rd,self.read32(a)),4=>self.write_reg(rd,self.read8(a) as u32),5=>{let v=self.read8(a) as u32;self.write_reg(rd,if v&0x80!=0{v|0xffff_ff00}else{v})},6=>{let v=self.read16(a) as u32;self.write_reg(rd,if v&0x8000!=0{v|0xffff_0000}else{v})},7=>self.write_reg(rd,self.read16(a) as u32),_=>unreachable!()}return None;}
        if raw&0xe000==0x6000{let load=raw&(1<<11)!=0;let byte=raw&(1<<12)!=0;let rd=(raw&7) as usize;let rb=((raw>>3)&7) as usize;let off=((raw>>6)&31) as u32*if byte{1}else{4};let a=self.read_reg(rb).wrapping_add(off);if load{self.write_reg(rd,if byte{self.read8(a) as u32}else{self.read32(a)})}else if byte{self.write8(a,self.read_reg(rd) as u8)}else{self.write32(a&!3,self.read_reg(rd))}return None;}
        if raw&0xf000==0x8000{let load=raw&(1<<11)!=0;let rd=(raw&7) as usize;let rb=((raw>>3)&7) as usize;let a=self.read_reg(rb).wrapping_add(((raw>>6)&31) as u32*2);if load{self.write_reg(rd,self.read16(a) as u32)}else{self.write16(a,self.read_reg(rd) as u16)}return None;}
        if raw&0xf000==0x9000{let load=raw&(1<<11)!=0;let rd=((raw>>8)&7) as usize;let a=self.read_reg(13).wrapping_add(u32::from(raw&255)*4);if load{self.write_reg(rd,self.read32(a))}else{self.write32(a&!3,self.read_reg(rd))}return None;}
        if raw&0xf000==0xa000{let rd=((raw>>8)&7) as usize;let base=if raw&(1<<11)!=0{self.read_reg(13)}else{self.read_reg(REG_PC)&!3};self.write_reg(rd,base.wrapping_add(u32::from(raw&255)*4));return None;}
        if raw&0xff80==0xb000{let imm=u32::from(raw&127)<<2;let sp=self.read_reg(13);self.write_reg(13,if raw&0x80!=0{sp.wrapping_sub(imm)}else{sp.wrapping_add(imm)});return None;}
        if raw&0xfe00==0xb400||raw&0xfe00==0xbc00{let load=raw&(1<<11)!=0;let extra=raw&(1<<8)!=0;let regs=(raw&255) as u8;if load{let mut a=self.read_reg(13);for r in 0..8usize{if regs&(1<<r)!=0{self.write_reg(r,self.read32(a));a=a.wrapping_add(4);}}if extra{let t=self.read32(a)&!1;self.write_reg(13,a.wrapping_add(4));self.write_reg(REG_PC,t);self.set_thumb(true);return Some((t,true));}self.write_reg(13,a)}else{let sp=self.read_reg(13).wrapping_sub((regs.count_ones()+u32::from(extra))*4);let mut a=sp;for r in 0..8usize{if regs&(1<<r)!=0{self.write32(a&!3,self.read_reg(r));a=a.wrapping_add(4);}}if extra{self.write32(a&!3,self.read_reg(REG_LR));}self.write_reg(13,sp)}return None;}
        if raw&0xf000==0xc000{let load=raw&(1<<11)!=0;let rb=((raw>>8)&7) as usize;let regs=(raw&255) as u8;let mut a=self.read_reg(rb);for r in 0..8usize{if regs&(1<<r)!=0{if load{self.write_reg(r,self.read32(a))}else{self.write32(a&!3,self.read_reg(r))}a=a.wrapping_add(4);}}self.write_reg(rb,a);return None;}
        if raw&0xf800==0xe000{return None;}
        Some(self.raise_exception(ExceptionKind::Undefined))
    }
}
