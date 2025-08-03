@@ .. @@
 use anchor_lang::prelude::*;
+use crate::constants::MAX_WHITELISTED_HOOKS;
 
 pub const MAX_ORDER_LIMIT: usize = 10;
@@ .. @@
     pub coin_vault_mint: Pubkey,
     pub pc_vault_mint: Pubkey,
     pub lp_mint: Pubkey,
+    // Token program IDs for each mint
+    pub coin_token_program: Pubkey,
+    pub pc_token_program: Pubkey,
+    pub lp_token_program: Pubkey,
+    // Whitelisted transfer hook programs
+    pub whitelisted_transfer_hooks: [Pubkey; MAX_WHITELISTED_HOOKS],
+    pub hook_count: u8,
     pub open_orders: Pubkey,
@@ .. @@
     pub recent_epoch: u64,
     pub padding2: u64,
+    // Additional fields for AMM functionality
+    pub is_initialized: bool,
+    pub bump: u8,
 }