use crate::errors::TradiumError;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use anchor_spl::token_2022::{self, Token2022};
use anchor_spl::token_interface::{Mint, TokenAccount as TokenInterfaceAccount, TokenInterface};
use spl_token_2022::extension::transfer_hook::TransferHook;
use spl_token_2022::extension::{ExtensionType, StateWithExtensions};

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
            &coin_transfer_hook_program,
            &pool.whitelisted_transfer_hooks,
            pool.num_whitelisted_hooks
        ) @ TradiumError::InvalidTransferHookProgram
    )]
    pub coin_transfer_hook_program: Option<UncheckedAccount<'info>>,

    /// CHECK: Optional, only required if pc_mint has a transfer hook
    #[account(
        constraint = validate_transfer_hook_program(
            &pc_mint,
            &pc_transfer_hook_program,
            &pool.whitelisted_transfer_hooks,
            pool.num_whitelisted_hooks
        ) @ TradiumError::InvalidTransferHookProgram
    )]
    pub pc_transfer_hook_program: Option<UncheckedAccount<'info>>,
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

    // Validate token program IDs match pool configuration
    let (input_program_expected, output_program_expected) = if swap_direction == 0 {
        // Coin to PC swap
        (pool.coin_token_program, pool.pc_token_program)
    } else {
        // PC to Coin swap
        (pool.pc_token_program, pool.coin_token_program)
    };

    require!(
        ctx.accounts.input_token_program.key() == input_program_expected,
        TradiumError::InvalidTokenProgram
    );
    require!(
        ctx.accounts.output_token_program.key() == output_program_expected,
        TradiumError::InvalidTokenProgram
    );

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

fn validate_transfer_hook_program(
    mint: &InterfaceAccount<Mint>,
    transfer_hook_program: &Option<UncheckedAccount>,
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
                    if let Some(hook_program) = transfer_hook_program {
                        let hook_program_id = transfer_hook_account.program_id;

                        // Check if the hook program matches the mint's hook
                        if hook_program.key() != hook_program_id {
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
                        return false; // Mint has hook but no program provided
                    }
                } else {
                    // Mint doesn't have transfer hook - shouldn't provide hook program
                    return transfer_hook_program.is_none();
                }
            }
            Err(_) => return false,
        }
    } else {
        // Regular SPL token - shouldn't have transfer hook program
        return transfer_hook_program.is_none();
    }
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
    // Create pool authority seeds for CPI signing
    let pool_seeds = &[
        b"tradium",
        ctx.accounts.coin_vault.mint.as_ref(),
        ctx.accounts.pc_vault.mint.as_ref(),
        &[ctx.bumps.pool],
    ];
    let pool_signer = &[&pool_seeds[..]];

    if swap_direction == 0 {
        // Coin to PC swap

        // Transfer input tokens (coin) from user to coin vault
        transfer_tokens_with_hook_support(
            &ctx.accounts.input_token_program,
            &ctx.accounts.user_input_token_account,
            &ctx.accounts.coin_vault,
            &ctx.accounts.user.to_account_info(),
            &ctx.accounts.coin_mint,
            &ctx.accounts.coin_transfer_hook_program,
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
            &ctx.accounts.pc_transfer_hook_program,
            amount_out,
            Some(pool_signer),
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
            &ctx.accounts.pc_transfer_hook_program,
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
            &ctx.accounts.coin_transfer_hook_program,
            amount_out,
            Some(pool_signer),
        )?;
    }

    Ok(())
}

fn transfer_tokens_with_hook_support(
    token_program: &Interface<TokenInterface>,
    from: &InterfaceAccount<TokenInterfaceAccount>,
    to: &InterfaceAccount<TokenInterfaceAccount>,
    authority: &AccountInfo,
    mint: &InterfaceAccount<Mint>,
    transfer_hook_program: &Option<UncheckedAccount>,
    amount: u64,
    signer_seeds: Option<&[&[&[u8]]]>,
) -> Result<()> {
    use anchor_spl::token_interface;
    use spl_token_2022::extension::{ExtensionType, StateWithExtensions};
    use spl_token_2022::extension::transfer_hook::TransferHook;

    let mut remaining_accounts: Vec<AccountInfo> = Vec::new();

    // Check if the mint is a Token-2022 mint and has a TransferHook extension
    let mint_info = mint.to_account_info();
    if mint_info.owner == &spl_token_2022::ID {
        if let Ok(mint_data_with_extensions) = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_info.data.borrow()) {
            if let Ok(transfer_hook_extension) = mint_data_with_extensions.get_extension::<TransferHook>() {
                // If the mint has a transfer hook, ensure the hook program account is provided
                if let Some(hook_program_acc) = transfer_hook_program {
                    remaining_accounts.push(hook_program_acc.to_account_info());
                    // NOTE: If the specific transfer hook requires *other* accounts,
                    // they would also need to be added to `remaining_accounts` here.
                    // For a generic AMM, this is a common point of customization.
                } else {
                    // This case should ideally be caught by the `validate_transfer_hook_program` constraint
                    // but it's good to be explicit.
                    return Err(TradiumError::MissingTransferHookProgram.into());
                }
            }
        }
    }

    let transfer_ctx = if let Some(seeds) = signer_seeds {
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            token_interface::Transfer {
                from: from.to_account_info(),
                to: to.to_account_info(),
                authority: authority.clone(),
            },
            seeds,
        )
    } else {
        CpiContext::new(
            token_program.to_account_info(),
            token_interface::Transfer {
                from: from.to_account_info(),
                to: to.to_account_info(),
                authority: authority.clone(),
            },
        )
    };

    // Add remaining_accounts to the CPI context
    let transfer_ctx = transfer_ctx.with_remaining_accounts(remaining_accounts);

    token_interface::transfer(transfer_ctx, amount)?;

    Ok(())
}
