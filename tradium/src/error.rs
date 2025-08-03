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
    
    #[msg("Invalid Pool State")]
    InvalidPoolState,
    
    #[msg("Unauthorized")]
    Unauthorized,
    
    #[msg("Math Overflow")]
    MathOverflow,
    
    #[msg("Invalid Amount")]
    InvalidAmount,
}