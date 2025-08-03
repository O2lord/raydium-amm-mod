use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};
use anchor_spl::token_2022::{self, Token2022};
use anchor_spl::token_interface::{
    Mint as MintInterface, TokenAccount as TokenAccountInterface, TokenInterface,
};

use crate::error::TradiumError;
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

    /// Optional: Coin transfer hook program (for Token2022)
    /// CHECK: Validated in instruction logic if needed
    pub coin_transfer_hook_program: Option<UncheckedAccount<'info>>,

    /// Optional: PC transfer hook program (for Token2022)
    /// CHECK: Validated in instruction logic if needed
    pub pc_transfer_hook_program: Option<UncheckedAccount<'info>>,
}

pub fn withdraw(ctx: Context<Withdraw>, lp_amount: u64) -> Result<()> {
    let pool = &mut ctx.accounts.pool;

    // Validate minimum withdrawal amount
    require!(lp_amount > 0, TradiumError::InvalidAmount);

    // Validate user has sufficient LP tokens
    require!(
        ctx.accounts.user_lp_account.amount >= lp_amount,
        TradiumError::InsufficientBalance
    );

    // Validate token program IDs
    let coin_is_token2022 = ctx.accounts.coin_token_program_id.key() == token_2022::ID;
    let pc_is_token2022 = ctx.accounts.pc_token_program_id.key() == token_2022::ID;
    let lp_is_token2022 = ctx.accounts.lp_token_program_id.key() == token_2022::ID;

    require!(
        ctx.accounts.coin_token_program_id.key() == token::ID || coin_is_token2022,
        TradiumError::InvalidTokenProgram
    );
    require!(
        ctx.accounts.pc_token_program_id.key() == token::ID || pc_is_token2022,
        TradiumError::InvalidTokenProgram
    );
    require!(
        ctx.accounts.lp_token_program_id.key() == token::ID || lp_is_token2022,
        TradiumError::InvalidTokenProgram
    );

    // Validate transfer hook programs if Token2022 is used
    if coin_is_token2022 {
        require!(
            ctx.accounts.coin_transfer_hook_program.is_some(),
            TradiumError::MissingTransferHookProgram
        );
    }
    if pc_is_token2022 {
        require!(
            ctx.accounts.pc_transfer_hook_program.is_some(),
            TradiumError::MissingTransferHookProgram
        );
    }

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

    // Create pool authority seeds for CPI signing
    let pool_seeds = &[
        b"tradium",
        ctx.accounts.coin_vault_mint.key().as_ref(),
        ctx.accounts.pc_vault_mint.key().as_ref(),
        &[ctx.bumps.pool],
    ];
    let pool_signer = &[&pool_seeds[..]];

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

    // Transfer coin tokens from vault to user
    if coin_is_token2022 && ctx.accounts.coin_transfer_hook_program.is_some() {
        // Token2022 transfer with hook
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.coin_token_program_id.to_account_info(),
            Transfer {
                from: ctx.accounts.coin_vault.to_account_info(),
                to: ctx.accounts.user_coin_account.to_account_info(),
                authority: pool.to_account_info(),
            },
            pool_signer,
        )
        .with_remaining_accounts(vec![ctx
            .accounts
            .coin_transfer_hook_program
            .as_ref()
            .unwrap()
            .to_account_info()]);
        token_2022::transfer(transfer_ctx, coin_amount)?;
    } else {
        // Regular Token or Token2022 without hook
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.coin_token_program_id.to_account_info(),
            Transfer {
                from: ctx.accounts.coin_vault.to_account_info(),
                to: ctx.accounts.user_coin_account.to_account_info(),
                authority: pool.to_account_info(),
            },
            pool_signer,
        );
        token::transfer(transfer_ctx, coin_amount)?;
    }

    // Transfer PC tokens from vault to user
    if pc_is_token2022 && ctx.accounts.pc_transfer_hook_program.is_some() {
        // Token2022 transfer with hook
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.pc_token_program_id.to_account_info(),
            Transfer {
                from: ctx.accounts.pc_vault.to_account_info(),
                to: ctx.accounts.user_pc_account.to_account_info(),
                authority: pool.to_account_info(),
            },
            pool_signer,
        )
        .with_remaining_accounts(vec![ctx
            .accounts
            .pc_transfer_hook_program
            .as_ref()
            .unwrap()
            .to_account_info()]);
        token_2022::transfer(transfer_ctx, pc_amount)?;
    } else {
        // Regular Token or Token2022 without hook
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.pc_token_program_id.to_account_info(),
            Transfer {
                from: ctx.accounts.pc_vault.to_account_info(),
                to: ctx.accounts.user_pc_account.to_account_info(),
                authority: pool.to_account_info(),
            },
            pool_signer,
        );
        token::transfer(transfer_ctx, pc_amount)?;
    }

    // Update pool state
    pool.lp_amount = pool
        .lp_amount
        .checked_sub(lp_amount)
        .ok_or(TradiumError::MathOverflow)?;

    // Emit withdrawal event
    emit!(WithdrawalEvent {
        pool: pool.key(),
        user: ctx.accounts.user_authority.key(),
        lp_amount,
        coin_amount,
        pc_amount,
        timestamp: Clock::get()?.unix_timestamp,
    });

    msg!(
        "Withdrawal completed: LP burned: {}, Coin withdrawn: {}, PC withdrawn: {}",
        lp_amount,
        coin_amount,
        pc_amount
    );

    Ok(())
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
