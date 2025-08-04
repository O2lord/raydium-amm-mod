use anchor_lang::prelude::*;

pub const MAX_ORDER_LIMIT: usize = 10;
pub const MAX_WHITELISTED_HOOKS: usize = 10;

#[account]
#[derive(Default, PartialEq)]
pub struct Tradium {
    pub status: u64,
    pub nonce: [u8; 1],
    pub order_num: u64,
    pub depth: u64,
    pub coin_decimals: u64,
    pub pc_decimals: u64,
    pub state: u64,
    pub reset_flag: u64,
    pub min_size: u64,
    pub vol_max_cut_ratio: u64,
    pub amount_wave: u64,
    pub coin_lot_size: u64,
    pub pc_lot_size: u64,
    pub min_price_multiplier: u64,
    pub max_price_multiplier: u64,
    pub sys_decimal_value: u64,
    pub fees: Fees,
    pub state_data: StateData,
    pub coin_vault: Pubkey,
    pub pc_vault: Pubkey,
    pub coin_vault_mint: Pubkey,
    pub pc_vault_mint: Pubkey,
    pub lp_mint: Pubkey,
    pub open_orders: Pubkey,
    pub market: Pubkey,
    pub market_program: Pubkey,
    pub target_orders: Pubkey,
    // Add token program fields
    pub coin_token_program: Pubkey,
    pub pc_token_program: Pubkey,
    // Add whitelisted transfer hooks
    pub whitelisted_transfer_hooks: [Pubkey; MAX_WHITELISTED_HOOKS],
    pub num_whitelisted_hooks: u8,
    pub padding1: [u64; 6], // Reduced padding to accommodate new fields
    pub amm_owner: Pubkey,
    pub lp_amount: u64,
    pub client_order_id: u64,
    pub recent_epoch: u64,
    pub padding2: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, PartialEq)]
pub struct Fees {
    pub min_separate_numerator: u64,
    pub min_separate_denominator: u64,
    pub trade_fee_numerator: u64,
    pub trade_fee_denominator: u64,
    pub pnl_numerator: u64,
    pub pnl_denominator: u64,
    pub swap_fee_numerator: u64,
    pub swap_fee_denominator: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default, PartialEq)]
pub struct StateData {
    pub initialized: bool,
    pub nonce: u8,
    pub coin_decimals: u8,
    pub pc_decimals: u8,
    pub state: u32,
    pub reset_flag: u32,
    pub pool_coin_token_account: Pubkey,
    pub pool_pc_token_account: Pubkey,
    pub coin_mint_key: Pubkey,
    pub pc_mint_key: Pubkey,
    pub lp_mint_key: Pubkey,
    pub pool_withdraw_queue: Pubkey,
    pub pool_temp_lp_token_account: Pubkey,
    pub serum_program_id: Pubkey,
    pub serum_market: Pubkey,
    pub serum_bids: Pubkey,
    pub serum_asks: Pubkey,
    pub serum_event_queue: Pubkey,
    pub serum_coin_vault_account: Pubkey,
    pub serum_pc_vault_account: Pubkey,
    pub serum_vault_signer: Pubkey,
    pub official_trade_label: Pubkey,
    pub swap_coin_in_amount: u64,
    pub swap_pc_out_amount: u64,
    pub swap_coin_to_pc_fee: u64,
    pub swap_pc_in_amount: u64,
    pub swap_coin_out_amount: u64,
    pub swap_pc_to_coin_fee: u64,
    pub pool_total_deposit_pc: u64,
    pub pool_total_deposit_coin: u64,
    pub swap_coin_in_amount_total: u64,
    pub swap_pc_out_amount_total: u64,
    pub swap_pc_in_amount_total: u64,
    pub swap_coin_out_amount_total: u64,
    pub swap_coin_to_pc_fee_total: u64,
    pub swap_pc_to_coin_fee_total: u64,
    pub pool_coin_amount: u64,
    pub pool_pc_amount: u64,
    pub pool_lp_amount: u64,
    pub padding: [u64; 3],
}

#[derive(Clone, Copy)]
pub struct TargetOrder {
    pub price: u64,
    pub coin_qty: u64,
    pub pc_qty: u64,
    pub client_id: u64,
}

#[derive(Clone, Copy)]
pub struct TargetOrders {
    pub owner: [u64; 4],
    pub buy_orders: [TargetOrder; 50],
    pub padding1: [u64; 8],
    pub target_x: u128,
    pub target_y: u128,
    pub plan_x_buy: u128,
    pub plan_y_buy: u128,
    pub plan_x_sell: u128,
    pub plan_y_sell: u128,
    pub placed_x: u128,
    pub placed_y: u128,
    pub calc_pnl_x: u128,
    pub calc_pnl_y: u128,
    pub sell_orders: [TargetOrder; 50],
    pub padding2: [u64; 6],
    pub replace_buy_client_id: [u64; MAX_ORDER_LIMIT],
    pub replace_sell_client_id: [u64; MAX_ORDER_LIMIT],
    pub last_order_numerator: u64,
    pub last_order_denominator: u64,
    pub plan_orders_cur: u64,
    pub place_orders_cur: u64,
    pub valid_buy_order_num: u64,
    pub valid_sell_order_num: u64,
    pub padding3: [u64; 10],
    pub free_slot_bits: u128,
}
