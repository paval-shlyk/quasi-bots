use crate::expenses::NativeCurrency;

pub enum WalletKind {
    Bank,
    Cache,
    Broker,
    Crypto,
}

pub struct Wallet {
    pub kind: WalletKind,
    pub amount: NativeCurrency,
}
