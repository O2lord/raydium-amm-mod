use anchor_lang::prelude::*;

// Program IDs for token programs
pub const SPL_TOKEN_PROGRAM_ID: Pubkey = spl_token::ID;
pub const SPL_TOKEN_2022_PROGRAM_ID: Pubkey = spl_token_2022::ID;

// Seeds for PDA generation
pub const POOL_SEED: &[u8] = b"pool";
pub const LP_MINT_SEED: &[u8] = b"lp_mint";
pub const VAULT_SEED: &[u8] = b"vault";

// Pool configuration
pub const MAX_WHITELISTED_HOOKS: usize = 10;
pub const MIN_LIQUIDITY: u64 = 1000; // Minimum liquidity to prevent division by zero
pub const FEE_DENOMINATOR: u64 = 10000; // For percentage calculations (0.01% = 1/10000)

// Default fees (in basis points)
pub const DEFAULT_TRADE_FEE: u64 = 30; // 0.3%
pub const DEFAULT_OWNER_FEE: u64 = 5; // 0.05%
