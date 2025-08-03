use crate::errors::TradiumError;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use anchor_spl::token_2022::{self, Token2022};
use anchor_spl::token_interface::{Mint, TokenAccount as TokenInterfaceAccount, TokenInterface};

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// The AMM pool state
    #[account(
        mut,
        seeds = [b"tradium", coin_vault.mint.as_ref(), pc_vault.mint.as_ref()],
        bump
    )]
    pub pool: Account<'info, Tradium>,

    /// User's input token account
    #[account(mut)]
    pub user_input_token_account: InterfaceAccount<'info, TokenInterfaceAccount>,

    /// User's output token account
    #[account(mut)]
    pub user_output_token_account: InterfaceAccount<'info, TokenInterfaceAccount>,

    /// Pool's coin vault (token A)
    #[account(
        mut,
        constraint = coin_vault.key() == pool.coin_vault
    )]
    pub coin_vault: InterfaceAccount<'info, TokenInterfaceAccount>,

    /// Pool's PC vault (token B)
    #[account(
        mut,
        constraint = pc_vault.key() == pool.pc_vault
    )]
    pub pc_vault: InterfaceAccount<'info, TokenInterfaceAccount>,

    /// Coin mint (token A)
    #[account(constraint = coin_mint.key() == pool.coin_vault_mint)]
    pub coin_mint: InterfaceAccount<'info, Mint>,

    /// PC mint (token B)
    #[account(constraint = pc_mint.key() == pool.pc_vault_mint)]
    pub pc_mint: InterfaceAccount<'info, Mint>,

    /// Token program for input token
    pub input_token_program: Interface<'info, TokenInterface>,

    /// Token program for output token  
    pub output_token_program: Interface<'info, TokenInterface>,

    /// Optional transfer hook program for input token (Token-2022)
    /// CHECK: Validated in instruction logic
    #[account()]
    pub input_transfer_hook_program: Option<UncheckedAccount<'info>>,

    /// Optional transfer hook program for output token (Token-2022)
    /// CHECK: Validated in instruction logic
    #[account()]
    pub output_transfer_hook_program: Option<UncheckedAccount<'info>>,
}

pub fn swap(
    ctx: Context<Swap>,
    amount_in: u64,
    minimum_amount_out: u64,
    swap_direction: u8, // 0 = coin to pc, 1 = pc to coin
) -> Result<()> {
    let pool = &mut ctx.accounts.pool;

    // Validate swap direction
    require!(swap_direction <= 1, TradiumError::InvalidSwapDirection);

    // Validate minimum input amount
    require!(amount_in > 0, TradiumError::InvalidInputAmount);

    // Validate token program IDs
    validate_token_programs(&ctx, swap_direction)?;

    // Get vault balances before swap
    let coin_vault_balance = ctx.accounts.coin_vault.amount;
    let pc_vault_balance = ctx.accounts.pc_vault.amount;

    // Perform the swap based on direction
    let amount_out = if swap_direction == 0 {
        // Coin to PC swap
        swap_coin_to_pc(&ctx, amount_in, coin_vault_balance, pc_vault_balance, pool)?
    } else {
        // PC to Coin swap
        swap_pc_to_coin(&ctx, amount_in, coin_vault_balance, pc_vault_balance, pool)?
    };

    // Check slippage protection
    require!(
        amount_out >= minimum_amount_out,
        TradiumError::SlippageExceeded
    );

    // Perform token transfers
    execute_swap_transfers(&ctx, amount_in, amount_out, swap_direction)?;

    // Update pool state
    pool.nonce = pool
        .nonce
        .checked_add(1)
        .ok_or(TradiumError::MathOverflow)?;

    msg!("Swap completed: {} -> {}", amount_in, amount_out);

    Ok(())
}

fn validate_token_programs(ctx: &Context<Swap>, swap_direction: u8) -> Result<()> {
    // Validate input token program
    let input_program_id = if swap_direction == 0 {
        &ctx.accounts.input_token_program.key()
    } else {
        &ctx.accounts.input_token_program.key()
    };

    require!(
        *input_program_id == Token::id() || *input_program_id == Token2022::id(),
        TradiumError::InvalidTokenProgram
    );

    // Validate output token program
    let output_program_id = &ctx.accounts.output_token_program.key();
    require!(
        *output_program_id == Token::id() || *output_program_id == Token2022::id(),
        TradiumError::InvalidTokenProgram
    );

    // Validate transfer hook programs if Token-2022
    if *input_program_id == Token2022::id() {
        if let Some(hook_program) = &ctx.accounts.input_transfer_hook_program {
            // Additional validation for transfer hook program can be added here
            msg!("Input transfer hook program: {}", hook_program.key());
        }
    }

    if *output_program_id == Token2022::id() {
        if let Some(hook_program) = &ctx.accounts.output_transfer_hook_program {
            // Additional validation for transfer hook program can be added here
            msg!("Output transfer hook program: {}", hook_program.key());
        }
    }

    Ok(())
}

fn swap_coin_to_pc(
    ctx: &Context<Swap>,
    amount_in: u64,
    coin_balance: u64,
    pc_balance: u64,
    pool: &Tradium,
) -> Result<u64> {
    // Calculate output amount using constant product formula (x * y = k)
    // with fees applied
    let fee_numerator = pool.fees.swap_fee_numerator;
    let fee_denominator = pool.fees.swap_fee_denominator;

    // Apply fee to input amount
    let amount_in_after_fee = amount_in
        .checked_mul(
            fee_denominator
                .checked_sub(fee_numerator)
                .ok_or(TradiumError::MathOverflow)?,
        )
        .ok_or(TradiumError::MathOverflow)?
        .checked_div(fee_denominator)
        .ok_or(TradiumError::MathOverflow)?;

    // Calculate output amount: amount_out = (amount_in_after_fee * pc_balance) / (coin_balance + amount_in_after_fee)
    let new_coin_balance = coin_balance
        .checked_add(amount_in_after_fee)
        .ok_or(TradiumError::MathOverflow)?;
    let amount_out = amount_in_after_fee
        .checked_mul(pc_balance)
        .ok_or(TradiumError::MathOverflow)?
        .checked_div(new_coin_balance)
        .ok_or(TradiumError::MathOverflow)?;

    // Ensure output amount doesn't exceed vault balance
    require!(
        amount_out <= pc_balance,
        TradiumError::InsufficientLiquidity
    );

    Ok(amount_out)
}

fn swap_pc_to_coin(
    ctx: &Context<Swap>,
    amount_in: u64,
    coin_balance: u64,
    pc_balance: u64,
    pool: &Tradium,
) -> Result<u64> {
    // Calculate output amount using constant product formula (x * y = k)
    // with fees applied
    let fee_numerator = pool.fees.swap_fee_numerator;
    let fee_denominator = pool.fees.swap_fee_denominator;

    // Apply fee to input amount
    let amount_in_after_fee = amount_in
        .checked_mul(
            fee_denominator
                .checked_sub(fee_numerator)
                .ok_or(TradiumError::MathOverflow)?,
        )
        .ok_or(TradiumError::MathOverflow)?
        .checked_div(fee_denominator)
        .ok_or(TradiumError::MathOverflow)?;

    // Calculate output amount: amount_out = (amount_in_after_fee * coin_balance) / (pc_balance + amount_in_after_fee)
    let new_pc_balance = pc_balance
        .checked_add(amount_in_after_fee)
        .ok_or(TradiumError::MathOverflow)?;
    let amount_out = amount_in_after_fee
        .checked_mul(coin_balance)
        .ok_or(TradiumError::MathOverflow)?
        .checked_div(new_pc_balance)
        .ok_or(TradiumError::MathOverflow)?;

    // Ensure output amount doesn't exceed vault balance
    require!(
        amount_out <= coin_balance,
        TradiumError::InsufficientLiquidity
    );

    Ok(amount_out)
}

fn execute_swap_transfers(
    ctx: &Context<Swap>,
    amount_in: u64,
    amount_out: u64,
    swap_direction: u8,
) -> Result<()> {
    if swap_direction == 0 {
        // Coin to PC swap

        // Transfer input tokens (coin) from user to coin vault
        let input_transfer_ctx = CpiContext::new(
            ctx.accounts.input_token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_input_token_account.to_account_info(),
                to: ctx.accounts.coin_vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        );

        if ctx.accounts.input_token_program.key() == Token2022::id() {
            // Handle Token-2022 transfer with optional hook
            if ctx.accounts.input_transfer_hook_program.is_some() {
                // Include transfer hook accounts if needed
                token_2022::transfer_checked(
                    input_transfer_ctx,
                    amount_in,
                    ctx.accounts.coin_mint.decimals,
                )?;
            } else {
                token_2022::transfer(input_transfer_ctx, amount_in)?;
            }
        } else {
            token::transfer(input_transfer_ctx, amount_in)?;
        }

        // Transfer output tokens (pc) from pc vault to user
        let pool_seeds = &[
            b"tradium",
            ctx.accounts.coin_vault.mint.as_ref(),
            ctx.accounts.pc_vault.mint.as_ref(),
            &[ctx.bumps.pool],
        ];
        let pool_signer = &[&pool_seeds[..]];

        let output_transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.output_token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.pc_vault.to_account_info(),
                to: ctx.accounts.user_output_token_account.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            pool_signer,
        );

        if ctx.accounts.output_token_program.key() == Token2022::id() {
            // Handle Token-2022 transfer with optional hook
            if ctx.accounts.output_transfer_hook_program.is_some() {
                token_2022::transfer_checked(
                    output_transfer_ctx,
                    amount_out,
                    ctx.accounts.pc_mint.decimals,
                )?;
            } else {
                token_2022::transfer(output_transfer_ctx, amount_out)?;
            }
        } else {
            token::transfer(output_transfer_ctx, amount_out)?;
        }
    } else {
        // PC to Coin swap

        // Transfer input tokens (pc) from user to pc vault
        let input_transfer_ctx = CpiContext::new(
            ctx.accounts.input_token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_input_token_account.to_account_info(),
                to: ctx.accounts.pc_vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        );

        if ctx.accounts.input_token_program.key() == Token2022::id() {
            if ctx.accounts.input_transfer_hook_program.is_some() {
                token_2022::transfer_checked(
                    input_transfer_ctx,
                    amount_in,
                    ctx.accounts.pc_mint.decimals,
                )?;
            } else {
                token_2022::transfer(input_transfer_ctx, amount_in)?;
            }
        } else {
            token::transfer(input_transfer_ctx, amount_in)?;
        }

        // Transfer output tokens (coin) from coin vault to user
        let pool_seeds = &[
            b"tradium",
            ctx.accounts.coin_vault.mint.as_ref(),
            ctx.accounts.pc_vault.mint.as_ref(),
            &[ctx.bumps.pool],
        ];
        let pool_signer = &[&pool_seeds[..]];

        let output_transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.output_token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.coin_vault.to_account_info(),
                to: ctx.accounts.user_output_token_account.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            pool_signer,
        );

        if ctx.accounts.output_token_program.key() == Token2022::id() {
            if ctx.accounts.output_transfer_hook_program.is_some() {
                token_2022::transfer_checked(
                    output_transfer_ctx,
                    amount_out,
                    ctx.accounts.coin_mint.decimals,
                )?;
            } else {
                token_2022::transfer(output_transfer_ctx, amount_out)?;
            }
        } else {
            token::transfer(output_transfer_ctx, amount_out)?;
        }
    }

    Ok(())
}
