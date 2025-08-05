use crate::error::TradiumError;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount as TokenInterfaceAccount, TokenInterface};
use spl_token_2022::extension::transfer_hook::TransferHook;
use spl_token_2022::extension::{BaseStateWithExtensions, ExtensionType, StateWithExtensions};

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

    /// CHECK: Optional, only required if coin_mint has a transfer hook
    #[account(
        constraint = validate_transfer_hook_program(
            &coin_mint,
            &coin_transfer_hook_program.to_account_info(),
            &pool.whitelisted_transfer_hooks,
            pool.num_whitelisted_hooks
        ) @ TradiumError::InvalidTransferHookProgram
    )]
    pub coin_transfer_hook_program: Option<UncheckedAccount<'info>>,

    /// CHECK: Optional, only required if pc_mint has a transfer hook
    #[account(
        constraint = validate_transfer_hook_program(
            &pc_mint,
            &pc_transfer_hook_program.to_account_info(),
            &pool.whitelisted_transfer_hooks,
            pool.num_whitelisted_hooks
        ) @ TradiumError::InvalidTransferHookProgram
    )]
    pub pc_transfer_hook_program: Option<UncheckedAccount<'info>>,
}

pub fn swap(
    mut ctx: Context<Swap>,
    amount_in: u64,
    min_amount_out: u64,
    swap_direction: u8,
) -> Result<()> {
    // Validate swap direction
    require!(swap_direction <= 1, TradiumError::InvalidSwapDirection);

    // Validate minimum input amount
    require!(amount_in > 0, TradiumError::InvalidInputAmount);

    // Validate token program IDs match pool configuration
    let (input_program_expected, output_program_expected) = if swap_direction == 0 {
        // Coin to PC swap
        (
            ctx.accounts.pool.coin_token_program,
            ctx.accounts.pool.pc_token_program,
        )
    } else {
        // PC to Coin swap
        (
            ctx.accounts.pool.pc_token_program,
            ctx.accounts.pool.coin_token_program,
        )
    };

    require!(
        ctx.accounts.input_token_program.key() == input_program_expected,
        TradiumError::InvalidTokenProgram
    );
    require!(
        ctx.accounts.output_token_program.key() == output_program_expected,
        TradiumError::InvalidTokenProgram
    );

    // Execute the swap with transfers and state updates
    execute_swap_transfers(ctx, amount_in, min_amount_out, swap_direction)?;

    Ok(())
}

fn execute_swap_transfers(
    mut ctx: Context<Swap>,
    amount_in: u64,
    minimum_amount_out: u64,
    swap_direction: u8,
) -> Result<()> {
    // Extract keys for seeds to avoid mutable borrow conflicts
    let coin_vault_mint_key = ctx.accounts.coin_mint.key();
    let pc_vault_mint_key = ctx.accounts.pc_mint.key();
    let pool_nonce_slice: &[u8] = &ctx.accounts.pool.nonce;

    // Get vault balances before swap
    let coin_vault_balance = ctx.accounts.coin_vault.amount;
    let pc_vault_balance = ctx.accounts.pc_vault.amount;

    // Calculate amount_out based on swap direction
    let amount_out = if swap_direction == 0 {
        // Coin to PC swap - inline swap_coin_to_pc logic
        let fee_numerator = ctx.accounts.pool.fees.swap_fee_numerator;
        let fee_denominator = ctx.accounts.pool.fees.swap_fee_denominator;

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
        let new_coin_balance = coin_vault_balance
            .checked_add(amount_in_after_fee)
            .ok_or(TradiumError::MathOverflow)?;
        let calculated_amount_out = amount_in_after_fee
            .checked_mul(pc_vault_balance)
            .ok_or(TradiumError::MathOverflow)?
            .checked_div(new_coin_balance)
            .ok_or(TradiumError::MathOverflow)?;

        // Ensure output amount doesn't exceed vault balance
        require!(
            calculated_amount_out <= pc_vault_balance,
            TradiumError::InsufficientLiquidity
        );

        calculated_amount_out
    } else {
        // PC to Coin swap - inline swap_pc_to_coin logic
        let fee_numerator = ctx.accounts.pool.fees.swap_fee_numerator;
        let fee_denominator = ctx.accounts.pool.fees.swap_fee_denominator;

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
        let new_pc_balance = pc_vault_balance
            .checked_add(amount_in_after_fee)
            .ok_or(TradiumError::MathOverflow)?;
        let calculated_amount_out = amount_in_after_fee
            .checked_mul(coin_vault_balance)
            .ok_or(TradiumError::MathOverflow)?
            .checked_div(new_pc_balance)
            .ok_or(TradiumError::MathOverflow)?;

        // Ensure output amount doesn't exceed vault balance
        require!(
            calculated_amount_out <= coin_vault_balance,
            TradiumError::InsufficientLiquidity
        );

        calculated_amount_out
    };

    // Check slippage protection
    require!(
        amount_out >= minimum_amount_out,
        TradiumError::SlippageExceeded
    );

    // Construct signer seeds for pool-initiated transfers
    let pool_seeds = &[
        b"tradium",
        coin_vault_mint_key.as_ref(),
        pc_vault_mint_key.as_ref(),
        pool_nonce_slice,
    ];
    let signer_seeds = &[&pool_seeds[..]];

    if swap_direction == 0 {
        // Coin to PC swap

        // Transfer input tokens (coin) from user to coin vault
        transfer_tokens_with_hook_support(
            &ctx.accounts.input_token_program,
            &ctx.accounts.user_input_token_account,
            &ctx.accounts.coin_vault,
            &ctx.accounts.user.to_account_info(),
            &ctx.accounts.coin_mint,
            ctx.accounts.coin_transfer_hook_program.as_ref(),
            amount_in,
            None,
        )?;

        // Transfer output tokens (pc) from pc vault to user
        transfer_tokens_with_hook_support(
            &ctx.accounts.output_token_program,
            &ctx.accounts.pc_vault,
            &ctx.accounts.user_output_token_account,
            &ctx.accounts.pool.to_account_info(),
            &ctx.accounts.pc_mint,
            ctx.accounts.pc_transfer_hook_program.as_ref(),
            amount_out,
            Some(signer_seeds),
        )?;
    } else {
        // PC to Coin swap

        // Transfer input tokens (pc) from user to pc vault
        transfer_tokens_with_hook_support(
            &ctx.accounts.input_token_program,
            &ctx.accounts.user_input_token_account,
            &ctx.accounts.pc_vault,
            &ctx.accounts.user.to_account_info(),
            &ctx.accounts.pc_mint,
            ctx.accounts.pc_transfer_hook_program.as_ref(),
            amount_in,
            None,
        )?;

        // Transfer output tokens (coin) from coin vault to user
        transfer_tokens_with_hook_support(
            &ctx.accounts.output_token_program,
            &ctx.accounts.coin_vault,
            &ctx.accounts.user_output_token_account,
            &ctx.accounts.pool.to_account_info(),
            &ctx.accounts.coin_mint,
            ctx.accounts.coin_transfer_hook_program.as_ref(),
            amount_out,
            Some(signer_seeds),
        )?;
    }

    // Update pool nonce
    ctx.accounts.pool.nonce[0] = ctx.accounts.pool.nonce[0]
        .checked_add(1)
        .ok_or(TradiumError::MathOverflow)?;

    msg!("Swap completed: {} -> {}", amount_in, amount_out);

    Ok(())
}

// Inline the transfer_tokens_with_hook_support function
fn transfer_tokens_with_hook_support<'info>(
    token_program: &Interface<'info, TokenInterface>,
    from: &InterfaceAccount<'info, TokenInterfaceAccount>,
    to: &InterfaceAccount<'info, TokenInterfaceAccount>,
    authority: &AccountInfo<'info>,
    mint: &InterfaceAccount<'info, Mint>,
    transfer_hook_program: Option<&UncheckedAccount<'info>>,
    amount: u64,
    signer_seeds: Option<&[&[&[u8]]]>,
) -> Result<()> {
    let mut remaining_accounts = Vec::new();

    // Check if mint has transfer hook extension
    if mint.to_account_info().owner == &spl_token_2022::ID {
        if let Ok(mint_data) = mint.to_account_info().try_borrow_data() {
            if let Ok(mint_with_extensions) =
                StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)
            {
                if let Ok(transfer_hook) = mint_with_extensions.get_extension::<TransferHook>() {
                    if let Some(hook_program_id) = Option::<Pubkey>::from(transfer_hook.program_id)
                    {
                        if let Some(hook_program) = transfer_hook_program {
                            require!(
                                hook_program.key() == hook_program_id,
                                TradiumError::InvalidTransferHookProgram
                            );
                            remaining_accounts.push(hook_program.to_account_info());
                        }
                    }
                }
            }
        }
    }

    let cpi_accounts = anchor_spl::token_interface::Transfer {
        from: from.to_account_info(),
        to: to.to_account_info(),
        authority: authority.clone(),
    };

    let cpi_ctx = if let Some(seeds) = signer_seeds {
        CpiContext::new_with_signer(token_program.to_account_info(), cpi_accounts, seeds)
            .with_remaining_accounts(remaining_accounts)
    } else {
        CpiContext::new(token_program.to_account_info(), cpi_accounts)
            .with_remaining_accounts(remaining_accounts)
    };

    anchor_spl::token_interface::transfer(cpi_ctx, amount)?;

    Ok(())
}

// Inline the validate_transfer_hook_program function
fn validate_transfer_hook_program(
    mint: &InterfaceAccount<Mint>,
    transfer_hook_program: &AccountInfo,
    whitelisted_hooks: &[Pubkey],
    num_whitelisted: u8,
) -> bool {
    // If no transfer hook program is provided, it's valid (no hook required)
    if transfer_hook_program.key() == Pubkey::default() {
        return true;
    }

    // Check if the mint actually has a transfer hook
    if mint.to_account_info().owner == &spl_token_2022::ID {
        if let Ok(mint_data) = mint.to_account_info().try_borrow_data() {
            if let Ok(mint_with_extensions) =
                StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)
            {
                if let Ok(transfer_hook) = mint_with_extensions.get_extension::<TransferHook>() {
                    if let Some(hook_program_id) = Option::<Pubkey>::from(transfer_hook.program_id)
                    {
                        // Verify the provided program matches the mint's hook
                        if transfer_hook_program.key() != hook_program_id {
                            return false;
                        }

                        // Check if the hook program is whitelisted
                        for i in 0..(num_whitelisted as usize) {
                            if i < whitelisted_hooks.len()
                                && whitelisted_hooks[i] == hook_program_id
                            {
                                return true;
                            }
                        }
                        return false; // Hook program not whitelisted
                    }
                }
            }
        }
    }

    // If we can't read the mint data or there's no hook, the program shouldn't be provided
    transfer_hook_program.key() == Pubkey::default()
}
