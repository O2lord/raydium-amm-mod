use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};
use anchor_spl::token_2022::{self, Token2022};
use anchor_spl::token_interface::{
    Mint as MintInterface, TokenAccount as TokenAccountInterface, TokenInterface,
};
use spl_token_2022::extension::transfer_hook::TransferHook;
use spl_token_2022::extension::{BaseStateWithExtensions, ExtensionType, StateWithExtensions};

use crate::error::TradiumError;
use crate::shared;
use crate::state::*;

#[derive(Accounts)]
pub struct Withdraw<'info> {
    /// The pool (Tradium) account
    #[account(
        mut,
        seeds = [
            b"tradium",
            coin_vault_mint.key().as_ref(),
            pc_vault_mint.key().as_ref()
        ],
        bump
    )]
    pub pool: Account<'info, Tradium>,

    /// User authority
    pub user_authority: Signer<'info>,

    /// User's LP token account
    #[account(
        mut,
        associated_token::mint = lp_mint,
        associated_token::authority = user_authority
    )]
    pub user_lp_account: InterfaceAccount<'info, TokenAccountInterface>,

    /// User's coin token account
    #[account(
        mut,
        token::mint = coin_vault_mint,
        token::authority = user_authority
    )]
    pub user_coin_account: InterfaceAccount<'info, TokenAccountInterface>,

    /// User's PC token account
    #[account(
        mut,
        token::mint = pc_vault_mint,
        token::authority = user_authority
    )]
    pub user_pc_account: InterfaceAccount<'info, TokenAccountInterface>,

    /// Pool's coin vault
    #[account(
        mut,
        address = pool.coin_vault
    )]
    pub coin_vault: InterfaceAccount<'info, TokenAccountInterface>,

    /// Pool's PC vault
    #[account(
        mut,
        address = pool.pc_vault
    )]
    pub pc_vault: InterfaceAccount<'info, TokenAccountInterface>,

    /// Coin mint
    #[account(address = pool.coin_vault_mint)]
    pub coin_vault_mint: InterfaceAccount<'info, MintInterface>,

    /// PC mint
    #[account(address = pool.pc_vault_mint)]
    pub pc_vault_mint: InterfaceAccount<'info, MintInterface>,

    /// LP mint
    #[account(
        mut,
        address = pool.lp_mint
    )]
    pub lp_mint: InterfaceAccount<'info, MintInterface>,

    /// LP token program (Token or Token2022)
    pub lp_token_program_id: Interface<'info, TokenInterface>,

    /// Coin token program (Token or Token2022)
    pub coin_token_program_id: Interface<'info, TokenInterface>,

    /// PC token program (Token or Token2022)
    pub pc_token_program_id: Interface<'info, TokenInterface>,

    /// CHECK: Optional, only required if coin_mint has a transfer hook
    #[account(
        constraint = validate_transfer_hook_program(
            &coin_vault_mint,
            &coin_transfer_hook_program.to_account_info(),
            &pool.whitelisted_transfer_hooks,
            pool.num_whitelisted_hooks
        ) @ TradiumError::InvalidTransferHookProgram
    )]
    pub coin_transfer_hook_program: Option<UncheckedAccount<'info>>,

    /// CHECK: Optional, only required if pc_mint has a transfer hook
    #[account(
        constraint = validate_transfer_hook_program(
            &pc_vault_mint,
            &pc_transfer_hook_program.to_account_info(),
            &pool.whitelisted_transfer_hooks,
            pool.num_whitelisted_hooks
        ) @ TradiumError::InvalidTransferHookProgram
    )]
    pub pc_transfer_hook_program: Option<UncheckedAccount<'info>>,
}

pub fn withdraw(ctx: Context<Withdraw>, lp_amount: u64) -> Result<()> {
    // Validate minimum withdrawal amount
    require!(lp_amount > 0, TradiumError::InvalidAmount);

    // Validate user has sufficient LP tokens
    require!(
        ctx.accounts.user_lp_account.amount >= lp_amount,
        TradiumError::InsufficientBalance
    );

    // Validate token program IDs match pool configuration
    require!(
        ctx.accounts.coin_token_program_id.key() == ctx.accounts.pool.coin_token_program,
        TradiumError::InvalidCoinTokenProgram
    );
    require!(
        ctx.accounts.pc_token_program_id.key() == ctx.accounts.pool.pc_token_program,
        TradiumError::InvalidPcTokenProgram
    );

    // Get current vault balances
    let coin_vault_balance = ctx.accounts.coin_vault.amount;
    let pc_vault_balance = ctx.accounts.pc_vault.amount;
    let total_lp_supply = ctx.accounts.lp_mint.supply;

    require!(total_lp_supply > 0, TradiumError::EmptyPool);

    // Calculate withdrawal amounts proportionally
    let coin_amount = (coin_vault_balance as u128)
        .checked_mul(lp_amount as u128)
        .ok_or(TradiumError::MathOverflow)?
        .checked_div(total_lp_supply as u128)
        .ok_or(TradiumError::MathOverflow)? as u64;

    let pc_amount = (pc_vault_balance as u128)
        .checked_mul(lp_amount as u128)
        .ok_or(TradiumError::MathOverflow)?
        .checked_div(total_lp_supply as u128)
        .ok_or(TradiumError::MathOverflow)? as u64;

    // Validate minimum withdrawal amounts
    require!(coin_amount > 0, TradiumError::InsufficientWithdrawal);
    require!(pc_amount > 0, TradiumError::InsufficientWithdrawal);

    // Burn LP tokens from user
    let burn_ctx = CpiContext::new(
        ctx.accounts.lp_token_program_id.to_account_info(),
        Burn {
            mint: ctx.accounts.lp_mint.to_account_info(),
            from: ctx.accounts.user_lp_account.to_account_info(),
            authority: ctx.accounts.user_authority.to_account_info(),
        },
    );
    token::burn(burn_ctx, lp_amount)?;
    // tradium/src/instructions/withdraw.rs
    // ... (inside the withdraw function)

    // Get pool account info before transfers to avoid borrow conflicts
    let pool_account_info = ctx.accounts.pool.to_account_info();

    // Obtain Pubkey references directly from the AccountInfo objects.
    // These references will have the same lifetime as the context ('info or '1).
    let coin_mint_key_ref: &[u8] = ctx.accounts.coin_vault_mint.to_account_info().key.as_ref();
    let pc_mint_key_ref: &[u8] = ctx.accounts.pc_vault_mint.to_account_info().key.as_ref();

    // Directly reference the bump from the pool account.
    // Since pool.nonce is now u8, &[ctx.accounts.pool.nonce] creates a &[u8]
    // that lives as long as ctx.accounts.pool, which is the '1 lifetime.
    let bump_seed_ref: &[u8] = &[ctx.accounts.pool.nonce];

    // Construct the PDA seeds array. All components now have the correct lifetimes.
    let pda_seeds_array: &[&[u8]] = &[
        b"tradium",        // Static slice, lives forever
        coin_mint_key_ref, // &'info [u8; 32] (correct lifetime)
        pc_mint_key_ref,   // &'info [u8; 32] (correct lifetime)
        bump_seed_ref,     // &'1 [u8; 1] (correct lifetime)
    ];

    // Create the final signer_seeds structure.
    // This is a reference to a stack-allocated array that contains the reference to our `pda_seeds_array`.
    let final_signer_seeds: &[&[&[u8]]] = &[pda_seeds_array];

    // Transfer coin tokens from vault to user with hook support
    shared::transfer_tokens_with_hook_support(
        &ctx.accounts.coin_token_program_id,
        &ctx.accounts.coin_vault,
        &ctx.accounts.user_coin_account,
        &pool_account_info,
        &ctx.accounts.coin_vault_mint,
        ctx.accounts.coin_transfer_hook_program.as_ref(),
        coin_amount,
        Some(final_signer_seeds),
    )?;

    // Transfer PC tokens from vault to user with hook support
    shared::transfer_tokens_with_hook_support(
        &ctx.accounts.pc_token_program_id,
        &ctx.accounts.pc_vault,
        &ctx.accounts.user_pc_account,
        &pool_account_info,
        &ctx.accounts.pc_vault_mint,
        ctx.accounts.pc_transfer_hook_program.as_ref(),
        pc_amount,
        Some(final_signer_seeds),
    )?;

    // ... (rest of the withdraw function)

    msg!(
        "Withdrawal completed: LP burned: {}, Coin withdrawn: {}, PC withdrawn: {}",
        lp_amount,
        coin_amount,
        pc_amount
    );

    Ok(())
}

fn validate_transfer_hook_program<'a>(
    mint: &InterfaceAccount<MintInterface>,
    transfer_hook_program: &'a AccountInfo<'a>,
    whitelisted_hooks: &[Pubkey],
    num_whitelisted: u8,
) -> bool {
    // Check if mint has transfer hook extension
    let mint_info = mint.to_account_info();
    let mint_data = mint_info.data.borrow();

    // For Token-2022 mints, check for transfer hook extension
    if mint_info.owner == &spl_token_2022::ID {
        match StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data) {
            Ok(mint_with_extensions) => {
                if let Ok(transfer_hook_account) =
                    mint_with_extensions.get_extension::<TransferHook>()
                {
                    // Mint has transfer hook - validate the provided program
                    let hook_program_id =
                        if let Some(pubkey) = transfer_hook_account.program_id.into() {
                            pubkey
                        } else {
                            return false;
                        };

                    // Check if the hook program matches the mint's hook
                    if transfer_hook_program.key() != hook_program_id {
                        return false;
                    }

                    // Check if the hook program is whitelisted
                    for i in 0..(num_whitelisted as usize) {
                        if whitelisted_hooks[i] == hook_program_id {
                            return true;
                        }
                    }
                    return false; // Hook program not whitelisted
                } else {
                    // Mint doesn't have transfer hook but program was provided - invalid
                    return false;
                }
            }
            Err(_) => return false,
        }
    } else {
        // Regular SPL token but transfer hook program was provided - invalid
        return false;
    }
}

#[event]
pub struct WithdrawalEvent {
    pub pool: Pubkey,
    pub user: Pubkey,
    pub lp_amount: u64,
    pub coin_amount: u64,
    pub pc_amount: u64,
    pub timestamp: i64,
}
