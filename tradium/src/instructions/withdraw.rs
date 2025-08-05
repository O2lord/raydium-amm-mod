use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Token, TokenAccount};
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
        constraint = shared::validate_transfer_hook_program(
            &coin_vault_mint,
            &coin_transfer_hook_program.to_account_info(),
            &pool.whitelisted_transfer_hooks,
            pool.num_whitelisted_hooks
        ) @ TradiumError::InvalidTransferHookProgram
    )]
    pub coin_transfer_hook_program: Option<UncheckedAccount<'info>>,

    /// CHECK: Optional, only required if pc_mint has a transfer hook
    #[account(
        constraint = shared::validate_transfer_hook_program(
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

    let pool_account_info = ctx.accounts.pool.to_account_info();

    let coin_mint_key_ref: &[u8] = ctx.accounts.coin_vault_mint.to_account_info().key.as_ref();
    let pc_mint_key_ref: &[u8] = ctx.accounts.pc_vault_mint.to_account_info().key.as_ref();
    let bump_seed_ref: &[u8] = &ctx.accounts.pool.nonce;

    // Define the common signer seeds for both transfers
    let cpi_seeds = &[
        &b"tradium"[..],
        coin_mint_key_ref,
        pc_mint_key_ref,
        bump_seed_ref,
    ];
    let signer_seeds = &[&cpi_seeds[..]];

    // --- Transfer coin tokens from vault to user with hook support ---
    let mut remaining_accounts_coin: Vec<AccountInfo> = Vec::new();
    if ctx.accounts.coin_vault_mint.to_account_info().owner == &spl_token_2022::ID {
        if let Ok(mint_data_with_extensions) =
            StateWithExtensions::<spl_token_2022::state::Mint>::unpack(
                &ctx.accounts.coin_vault_mint.to_account_info().data.borrow(),
            )
        {
            if let Ok(_transfer_hook_extension) =
                mint_data_with_extensions.get_extension::<TransferHook>()
            {
                if let Some(hook_program_acc) = ctx.accounts.coin_transfer_hook_program.as_ref() {
                    remaining_accounts_coin.push(hook_program_acc.to_account_info());
                } else {
                    return Err(TradiumError::InvalidTransferHookProgram.into());
                }
            }
        }
    }

    let transfer_accounts_coin = anchor_spl::token_interface::Transfer {
        from: ctx.accounts.coin_vault.to_account_info(),
        to: ctx.accounts.user_coin_account.to_account_info(),
        authority: pool_account_info.clone(),
    };

    let transfer_ctx_coin = CpiContext::new_with_signer(
        ctx.accounts.coin_token_program_id.to_account_info(),
        transfer_accounts_coin,
        signer_seeds,
    )
    .with_remaining_accounts(remaining_accounts_coin);

    anchor_spl::token_interface::transfer(transfer_ctx_coin, coin_amount)?;

    // --- Transfer PC tokens from vault to user with hook support ---
    let mut remaining_accounts_pc: Vec<AccountInfo> = Vec::new();
    if ctx.accounts.pc_vault_mint.to_account_info().owner == &spl_token_2022::ID {
        if let Ok(mint_data_with_extensions) =
            StateWithExtensions::<spl_token_2022::state::Mint>::unpack(
                &ctx.accounts.pc_vault_mint.to_account_info().data.borrow(),
            )
        {
            if let Ok(_transfer_hook_extension) =
                mint_data_with_extensions.get_extension::<TransferHook>()
            {
                if let Some(hook_program_acc) = ctx.accounts.pc_transfer_hook_program.as_ref() {
                    remaining_accounts_pc.push(hook_program_acc.to_account_info());
                } else {
                    return Err(TradiumError::InvalidTransferHookProgram.into());
                }
            }
        }
    }

    let transfer_accounts_pc = anchor_spl::token_interface::Transfer {
        from: ctx.accounts.pc_vault.to_account_info(),
        to: ctx.accounts.user_pc_account.to_account_info(),
        authority: pool_account_info,
    };

    let transfer_ctx_pc = CpiContext::new_with_signer(
        ctx.accounts.pc_token_program_id.to_account_info(),
        transfer_accounts_pc,
        signer_seeds,
    )
    .with_remaining_accounts(remaining_accounts_pc);

    anchor_spl::token_interface::transfer(transfer_ctx_pc, pc_amount)?;

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
                    return false;
                } else {
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
