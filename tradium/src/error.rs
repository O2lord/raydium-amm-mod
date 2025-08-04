use anchor_lang::prelude::*;

#[error_code]
pub enum TradiumError {
    #[msg("Unwhitelisted Transfer Hook Program")]
    UnwhitelistedTransferHookProgram,
    #[msg("Invalid Token-2022 Mint")]
    InvalidToken2022Mint,
    #[msg("Missing Transfer Hook Account")]
    MissingTransferHookAccount,
    #[msg("Invalid Token Program")]
    InvalidTokenProgram,
    #[msg("Insufficient Liquidity")]
    InsufficientLiquidity,
    #[msg("Slippage Tolerance Exceeded")]
    SlippageToleranceExceeded,
    #[msg("Invalid Transfer Hook Account")]
    InvalidTransferHookProgram,
    #[msg("Invalid Pool State")]
    InvalidPoolState,
    #[msg("Invalid coin mint")]
    InvalidCoinMint,
    #[msg("Invalid PC mint")]
    InvalidPcMint,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Math Overflow")]
    MathOverflow,
    #[msg("Invalid Amount")]
    InvalidAmount,
    #[msg("Invalid Deposit Amount")]
    InvalidDepositAmount,
    #[msg("Invalid Coin Token Program")]
    InvalidCoinTokenProgram,
    #[msg("Invalid PC Token Program")]
    InvalidPcTokenProgram,
    #[msg("Invalid Coin Vault")]
    InvalidCoinVault,
    #[msg("Invalid PC Vault")]
    InvalidPcVault,
    #[msg("Invalid LP Mint")]
    InvalidLpMint,
    #[msg("Insufficient Liquidity Minted")]
    InsufficientLiquidityMinted,
    #[msg("Insufficient Balance")]
    InsufficientBalance,
    #[msg("Empty Pool")]
    EmptyPool,
    #[msg("Insufficient Withdrawal")]
    InsufficientWithdrawal,
    #[msg("Missing Transfer Hook Program")]
    MissingTransferHookProgram,
    #[msg("Invalid Swap Direction")]
    InvalidSwapDirection,
    #[msg("Invalid Input Amount")]
    InvalidInputAmount,
    #[msg("Slippage Exceeded")]
    SlippageExceeded,
}
