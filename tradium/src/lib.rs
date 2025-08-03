@@ .. @@
 use anchor_lang::prelude::*;

 pub mod constants;
 pub use constants::*;

 pub mod error;
-use crate::error::TradiumError;
+pub use error::*;

 pub mod instructions;
 pub use instructions::*;

 pub mod state;
 pub use state::*;

 declare_id!("9cmTderZ6Sthr4UYNvc3sdLyZDDw5fnzed2PtnyP4ZF7");

 #[program]
 pub mod tradium {
     use super::*;

-    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
-        msg!("Greetings from: {:?}", ctx.program_id);
+    pub fn initialize_pool(
+        ctx: Context<InitializePool>,
+        bump: u8,
+        initial_coin_amount: u64,
+        initial_pc_amount: u64,
+    ) -> Result<()> {
+        let pool = &mut ctx.accounts.pool;
+        
+        // Validate token programs
+        let coin_program_id = ctx.accounts.coin_token_program.key();
+        let pc_program_id = ctx.accounts.pc_token_program.key();
+        
+        // Check if the provided token programs are valid
+        if coin_program_id != SPL_TOKEN_PROGRAM_ID && coin_program_id != SPL_TOKEN_2022_PROGRAM_ID {
+            return Err(TradiumError::InvalidTokenProgram.into());
+        }
+        
+        if pc_program_id != SPL_TOKEN_PROGRAM_ID && pc_program_id != SPL_TOKEN_2022_PROGRAM_ID {
+            return Err(TradiumError::InvalidTokenProgram.into());
+        }
+        
+        // Initialize pool state
+        pool.status = 1; // Active
+        pool.nonce = bump;
+        pool.coin_decimals = ctx.accounts.coin_mint.decimals as u64;
+        pool.pc_decimals = ctx.accounts.pc_mint.decimals as u64;
+        pool.sys_decimal_value = 10_u64.pow(6); // 6 decimal places for internal calculations
+        
+        // Set mint and vault addresses
+        pool.coin_vault_mint = ctx.accounts.coin_mint.key();
+        pool.pc_vault_mint = ctx.accounts.pc_mint.key();
+        pool.lp_mint = ctx.accounts.lp_mint.key();
+        pool.coin_vault = ctx.accounts.coin_vault.key();
+        pool.pc_vault = ctx.accounts.pc_vault.key();
+        
+        // Set token program IDs
+        pool.coin_token_program = coin_program_id;
+        pool.pc_token_program = pc_program_id;
+        pool.lp_token_program = ctx.accounts.token_program.key();
+        
+        // Initialize fees with default values
+        pool.fees.trade_fee_numerator = DEFAULT_TRADE_FEE;
+        pool.fees.trade_fee_denominator = FEE_DENOMINATOR;
+        pool.fees.swap_fee_numerator = DEFAULT_OWNER_FEE;
+        pool.fees.swap_fee_denominator = FEE_DENOMINATOR;
+        
+        // Initialize whitelisted transfer hooks (empty by default)
+        pool.whitelisted_transfer_hooks = [Pubkey::default(); MAX_WHITELISTED_HOOKS];
+        pool.hook_count = 0;
+        
+        // Set initialization flag
+        pool.is_initialized = true;
+        pool.bump = bump;
+        
+        msg!("Pool initialized successfully");
+        msg!("Coin mint: {}", ctx.accounts.coin_mint.key());
+        msg!("PC mint: {}", ctx.accounts.pc_mint.key());
+        msg!("LP mint: {}", ctx.accounts.lp_mint.key());
+        
         Ok(())
     }
 }

-#[derive(Accounts)]
-pub struct Initialize {}