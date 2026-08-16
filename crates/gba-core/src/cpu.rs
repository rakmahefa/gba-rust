use crate::Bus;

const N: u32 = 1<<31; const Z:u32=1<<30; const C:u32=1<<29; const V:u32=1<<28; const T:u32=1<<5;

pub struct Cpu { pub r:[u32;16], pub cpsr:u32, pub halted:bool }
impl Cpu {
    pub fn new()->Self { let mut c=Self{r:[0;16],cpsr:0x000000D3,halted:false}; c.r[13]=0x03007F00;c.r[15]=0x08000000;c }
    #[inline] fn thumb(&self)->bool{self.cpsr&T!=0}
    #[inline] fn set_nz(&mut self,v:u32){self.cpsr=(self.cpsr&!(N|Z))|if v&N!=0{N}else{0}|if v==0{Z}else{0}}
    #[inline] fn set_add_flags(&mut self,a:u32,b:u32,r:u32){let c=(r<a) as u32;let v=((!(a^b)&(a^r))>>31)&1;self.cpsr=(self.cpsr&!(N|Z|C|V))|if r&N!=0{N}else{0}|if r==0{Z}else{0}|c*C|v*V;}
    #[inline] fn set_sub_flags(&mut self,a:u32,b:u32,r:u32){let c=(a>=b) as u32;let v=(((a^b)&(a^r))>>31)&1;self.cpsr=(self.cpsr&!(N|Z|C|V))|if r&N!=0{N}else{0}|if r==0{Z}else{0}|c*C|v*V;}
    pub fn step(&mut self,bus:&mut Bus)->u32 { if self.halted{return 1}; if self.thumb(){self.step_thumb(bus)}else{self.step_arm(bus)} }
    fn step_arm(&mut self,bus:&mut Bus)->u32 {
        let pc=self.r[15]; let op=bus.read32(pc); self.r[15]=pc.wrapping_add(4);
        if op & 0x0ffffff0 == 0x012fff10 { let rm=(op&15) as usize; let t=self.r[rm]; self.cpsr=(self.cpsr&!T)|if t&1!=0{T}else{0}; self.r[15]=t&!1; return 3; }
        if op>>25 & 0x7 == 0b101 { let imm=(op&0x00ffffff)<<2; let off=((imm as i32)<<6>>6) as i32; if op&(1<<24)!=0{self.r[14]=pc.wrapping_add(4);} self.r[15]=((pc as i32).wrapping_add(4).wrapping_add(off)) as u32; return 3; }
        if op & 0x0f000000 == 0x0f000000 { return self.swi(bus,(op&0x00ffffff) as u8); }
        if op & 0x0c000000 == 0x04000000 { return self.arm_ldrstr(bus,op); }
        if op & 0x0e000000 == 0x08000000 { return self.arm_block(bus,op); }
        if op & 0x0c000000 == 0 { return self.arm_dp(op); }
        1
    }
    fn arm_dp(&mut self,op:u32)->u32 { let i=op>>25&1;let opcode=(op>>21)&15;let rn=((op>>16)&15)as usize;let rd=((op>>12)&15)as usize;let a=self.r[rn];let b=if i!=0{let imm=op&255;imm.rotate_right(((op>>8)&15)*2)}else{self.r[(op&15)as usize]};let oldc=self.cpsr&C!=0;let r=match opcode{0=>a&b,1=>a^b,2=>a.wrapping_sub(b),3=>b.wrapping_sub(a),4=>a.wrapping_add(b),5=>a.wrapping_add(b).wrapping_add(oldc as u32),6=>a.wrapping_sub(b).wrapping_sub(!oldc as u32),8=>{let r=a&b;self.set_nz(r);return 1},9=>{let r=a^b;self.set_nz(r);return 1},10=>{let r=a.wrapping_sub(b);self.set_sub_flags(a,b,r);return 1},11=>{let r=b.wrapping_sub(a);self.set_sub_flags(b,a,r);return 1},12=>a|b,13=>b,14=>a&!b,15=>!b,_=>0};self.r[rd]=r;if op&(1<<20)!=0{self.set_nz(r);}if rd==15{self.r[15]&=!3;}1}
    fn arm_ldrstr(&mut self,bus:&mut Bus,op:u32)->u32 {let rn=((op>>16)&15)as usize;let rd=((op>>12)&15)as usize;let mut off=if op&(1<<25)!=0{self.r[(op&15)as usize]}else{op&0xfff};if op&(1<<23)==0{off=!off+1;}let addr=self.r[rn].wrapping_add(off);let wb=op&(1<<22)!=0;let load=op&(1<<20)!=0;if load{self.r[rd]=if wb{bus.read8(addr)as u32}else{bus.read32(addr)}}else{if wb{bus.write8(addr,self.r[rd]as u8)}else{bus.write32(addr,self.r[rd])}};if op&(1<<21)!=0{self.r[rn]=addr;}2}
    fn arm_block(&mut self,bus:&mut Bus,op:u32)->u32 {let rn=((op>>16)&15)as usize;let mut addr=self.r[rn];let up=op&(1<<23)!=0;let before=op&(1<<24)!=0;let load=op&(1<<20)!=0;let regs=op&0xffff;let count=regs.count_ones();if up{if before{addr+=4;}}else{if before{addr-=4;}else{addr-=4*count;}}for i in 0..16{if regs&(1<<i)!=0{if load{self.r[i]=bus.read32(addr)}else{bus.write32(addr,self.r[i]);}addr=if up{addr+4}else{addr+4};}}if op&(1<<21)!=0{self.r[rn]=if up{self.r[rn]+4*count}else{self.r[rn]-4*count};}2+count}
    fn swi(&mut self,bus:&mut Bus,imm:u8)->u32 { match imm {0x00=>{},0x01=>{},0x02=>{},0x05=>{self.r[0]=0;},0x06=>{},0x0B=>{},_=>{}}; let _=bus; 3 }

    fn step_thumb(&mut self,bus:&mut Bus)->u32 { let pc=self.r[15];let op=bus.read16(pc);self.r[15]=pc+2;match op {
        0x0000..=0x07ff => {let imm=(op>>6)&31;let rs=((op>>3)&7)as usize;let rd=(op&7)as usize;let v=match (op>>11)&3{0=>self.r[rs]<<imm,1=>self.r[rs]>>imm,2=>(self.r[rs]as i32>>imm)as u32,_=>0};self.r[rd]=v;self.set_nz(v);1},
        0x1800..=0x1fff => {let sub=op&(1<<9)!=0;let rn=((op>>6)&7)as usize;let rs=((op>>3)&7)as usize;let rd=(op&7)as usize;let a=self.r[rs];let b=self.r[rn];let r=if sub{a.wrapping_sub(b)}else{a.wrapping_add(b)};self.r[rd]=r;if sub{self.set_sub_flags(a,b,r)}else{self.set_add_flags(a,b,r)};1},
        0x2000..=0x3fff => {let rd=((op>>8)&7)as usize;let imm=(op&255)as u32;let r=match (op>>11)&3{0=>imm,1=>self.r[rd].wrapping_add(imm),2=>self.r[rd].wrapping_sub(imm),_=>self.r[rd]};self.r[rd]=r;self.set_nz(r);1},
        0x4000..=0x43ff => {let rs=((op>>3)&7)as usize;let rd=(op&7)as usize;let b=self.r[rs];let a=self.r[rd];let code=(op>>6)&15;let r=match code{0=>a&b,1=>a^b,2=>a.wrapping_sub(b),3=>a.wrapping_add(b),4=>a<< (b&31),5=>a.rotate_right(b&31),6=>a>> (b&31),7=>(a as i32>>(b&31))as u32,8=>a&b,9=>0u32.wrapping_sub(b),10=>a.wrapping_sub(b),11=>a.wrapping_add(b),12=>a|b,13=>a.wrapping_mul(b),14=>a&!b,_=>!b};self.r[rd]=r;self.set_nz(r);1},
        0x4400..=0x47ff => {let h1=(op>>7)&1;let h2=(op>>6)&1;let rs=(((op>>3)&7)|((h2 as u16)<<3))as usize;let rd=((op&7)|((h1 as u16)<<3))as usize;if (op>>8)&3==3{let t=self.r[rs];self.cpsr=(self.cpsr&!T)|if t&1!=0{T}else{0};self.r[15]=t&!1}else{self.r[rd]=self.r[rd].wrapping_add(self.r[rs]);if rd==15{self.r[15]&=!1;}}1},
        0x4800..=0x4fff => {let rd=((op>>8)&7)as usize;let a=(self.r[15]&!2)+((op&255)as u32*4);self.r[rd]=bus.read32(a);2},
        0x5000..=0x5fff => {let ro=((op>>6)&7)as usize;let rb=((op>>3)&7)as usize;let rd=(op&7)as usize;let a=self.r[rb].wrapping_add(self.r[ro]);if op&(1<<12)!=0{bus.write32(a,self.r[rd])}else{self.r[rd]=bus.read32(a)};2},
        0x6000..=0x7fff => {let imm=((op>>6)&31)as u32;let rb=((op>>3)&7)as usize;let rd=(op&7)as usize;let a=self.r[rb]+imm*4;if op&(1<<11)!=0{self.r[rd]=bus.read32(a)}else{bus.write32(a,self.r[rd])};2},
        0xb400..=0xb5ff => {let regs=op&0xff;let mut sp=self.r[13];for i in 0..8{if regs&(1<<i)!=0{sp-=4;bus.write32(sp,self.r[i]);}}if op&0x100!=0{sp-=4;bus.write32(sp,self.r[14]);}self.r[13]=sp;2},
        0xbc00..=0xbdff => {let regs=op&0xff;let mut sp=self.r[13];for i in 0..8{if regs&(1<<i)!=0{self.r[i]=bus.read32(sp);sp+=4;}}if op&0x100!=0{self.r[15]=bus.read32(sp)&!1;sp+=4;}self.r[13]=sp;2},
        0xd000..=0xdfff => {let cond=(op>>8)&15;let off=((op&255)as i8 as i32)<<1;if cond==14{self.swi(bus,op as u8)}else if self.cond(cond as u8){self.r[15]=(self.r[15] as i32+off)as u32;1}else{1}},
        0xe000..=0xe7ff => {let off=(((op&0x7ff)<<1) as i32)<<20>>20;self.r[15]=(self.r[15] as i32+off)as u32;1},
        _=>1,
    }}
    fn cond(&self,c:u8)->bool{let z=self.cpsr&Z!=0;let n=self.cpsr&N!=0;let cv=self.cpsr&C!=0;let v=self.cpsr&V!=0;match c{0=>z,1=>!z,2=>cv,3=>!cv,4=>n,5=>!n,6=>v,7=>!v,8=>cv&&!z,9=>!cv||z,10=>n==v,11=>n!=v,12=>!z&&(n==v),13=>z||(n!=v),14=>true,_=>false}}
}
