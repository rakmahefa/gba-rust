#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Arm,
    Thumb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    Al,
    Eq,
    Ne,
    Cs,
    Cc,
    Mi,
    Pl,
    Vs,
    Vc,
    Hi,
    Ls,
    Ge,
    Lt,
    Gt,
    Le,
}

impl Condition {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Al => "",
            Self::Eq => "_eq",
            Self::Ne => "_ne",
            Self::Cs => "_cs",
            Self::Cc => "_cc",
            Self::Mi => "_mi",
            Self::Pl => "_pl",
            Self::Vs => "_vs",
            Self::Vc => "_vc",
            Self::Hi => "_hi",
            Self::Ls => "_ls",
            Self::Ge => "_ge",
            Self::Lt => "_lt",
            Self::Gt => "_gt",
            Self::Le => "_le",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchKind {
    Branch,
    Call,
    Exchange,
    Return,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand2 {
    Imm(u32),
    Reg {
        rm: u8,
        shift: u8,
        shift_kind: u8,
        by_register: bool,
        shift_register: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmDataOp {
    And,
    Eor,
    Sub,
    Rsb,
    Add,
    Adc,
    Sbc,
    Rsc,
    Tst,
    Teq,
    Cmp,
    Cmn,
    Orr,
    Mov,
    Bic,
    Mvn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmExtended {
    DataProcessing {
        op: ArmDataOp,
        rd: u8,
        rn: u8,
        op2: Operand2,
        set_flags: bool,
    },
    Multiply {
        rd: u8,
        rn: u8,
        rs: u8,
        rm: u8,
        accumulate: bool,
        set_flags: bool,
    },
    MultiplyLong {
        rd_hi: u8,
        rd_lo: u8,
        rs: u8,
        rm: u8,
        signed: bool,
        accumulate: bool,
        set_flags: bool,
    },
    Swap {
        rd: u8,
        rn: u8,
        rm: u8,
        byte: bool,
    },
    HalfwordTransfer {
        load: bool,
        signed: bool,
        halfword: bool,
        rd: u8,
        rn: u8,
        offset: i32,
        pre_index: bool,
        up: bool,
        write_back: bool,
    },
    SingleDataTransfer {
        load: bool,
        byte: bool,
        rd: u8,
        rn: u8,
        offset: Operand2,
        pre_index: bool,
        up: bool,
        write_back: bool,
    },
    BlockTransfer {
        load: bool,
        rn: u8,
        register_list: u16,
        pre_index: bool,
        up: bool,
        write_back: bool,
        user_mode: bool,
    },
    Mrs {
        rd: u8,
        spsr: bool,
    },
    Msr {
        spsr: bool,
        field_mask: u8,
        source: Operand2,
    },
    SoftwareInterrupt {
        comment: u32,
    },
    CoprocessorTransfer {
        load: bool,
        cp: u8,
        opcode1: u8,
        crd: u8,
        crn: u8,
        crm: u8,
        opcode2: u8,
        long: bool,
    },
    CoprocessorData {
        cp: u8,
        opcode1: u8,
        crd: u8,
        crn: u8,
        crm: u8,
        opcode2: u8,
    },
    CoprocessorRegisterTransfer {
        to_arm: bool,
        cp: u8,
        opcode1: u8,
        rd: u8,
        crn: u8,
        crm: u8,
        opcode2: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmOp {
    Nop,
    Mov {
        rd: u8,
        op2: Operand2,
    },
    Add {
        rd: u8,
        rn: u8,
        op2: Operand2,
    },
    Sub {
        rd: u8,
        rn: u8,
        op2: Operand2,
    },
    Cmp {
        rn: u8,
        op2: Operand2,
    },
    Load {
        rd: u8,
        rn: u8,
        offset: i32,
        byte: bool,
    },
    Store {
        rd: u8,
        rn: u8,
        offset: i32,
        byte: bool,
    },
    Branch {
        target: u32,
        condition: Condition,
        link: bool,
    },
    BranchExchange {
        rm: u8,
        link: bool,
    },
    Extended(ArmExtended),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbAluOp {
    And,
    Eor,
    Lsl,
    Lsr,
    Asr,
    Adc,
    Sbc,
    Ror,
    Tst,
    Neg,
    Cmp,
    Cmn,
    Orr,
    Mul,
    Bic,
    Mvn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbExtended {
    MoveShifted {
        kind: u8,
        rd: u8,
        rs: u8,
        offset: u8,
    },
    AddSubRegister {
        sub: bool,
        rd: u8,
        rs: u8,
        rn: u8,
    },
    AddSubImmediate {
        sub: bool,
        rd: u8,
        rs: u8,
        imm: u8,
    },
    Alu {
        op: ThumbAluOp,
        rd: u8,
        rs: u8,
    },
    HighRegister {
        op: u8,
        rd: u8,
        rs: u8,
    },
    PcRelativeLoad {
        rd: u8,
        word_offset: u8,
    },
    LoadStoreRegister {
        load: bool,
        byte: bool,
        rd: u8,
        rb: u8,
        ro: u8,
    },
    LoadStoreSignHalf {
        kind: u8,
        rd: u8,
        rb: u8,
        ro: u8,
    },
    LoadStoreImmediate {
        load: bool,
        byte: bool,
        rd: u8,
        rb: u8,
        offset: u8,
    },
    LoadStoreHalfword {
        load: bool,
        rd: u8,
        rb: u8,
        offset: u8,
    },
    SpRelativeLoadStore {
        load: bool,
        rd: u8,
        offset: u8,
    },
    Address {
        rd: u8,
        use_sp: bool,
        word_offset: u8,
    },
    AddSp {
        negative: bool,
        imm: u16,
    },
    PushPop {
        load: bool,
        registers: u8,
        extra_lr_pc: bool,
    },
    MultipleLoadStore {
        load: bool,
        rb: u8,
        register_list: u8,
    },
    SoftwareInterrupt {
        comment: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbOp {
    Nop,
    MovImm { rd: u8, imm: u8 },
    AddImm { rd: u8, rn: u8, imm: u8 },
    SubImm { rd: u8, rn: u8, imm: u8 },
    LoadImm { rd: u8, rn: u8, word_offset: u8 },
    StoreImm { rd: u8, rn: u8, word_offset: u8 },
    Branch { target: u32, condition: Condition },
    BranchLink { target: u32 },
    BranchExchange { rm: u8 },
    Extended(ThumbExtended),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionKind {
    Arm(ArmOp),
    Thumb(ThumbOp),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    pub address: u32,
    pub mode: Mode,
    pub raw: u32,
    pub size: u8,
    pub condition: Condition,
    pub kind: InstructionKind,
}
