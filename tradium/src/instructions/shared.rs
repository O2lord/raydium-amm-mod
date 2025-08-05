use crate::error::TradiumError;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    Mint as MintInterface, TokenAccount as TokenAccountInterface, TokenInterface,
};
use spl_token_2022::extension::transfer_hook::TransferHook;
use spl_token_2022::extension::{BaseStateWithExtensions, ExtensionType, StateWithExtensions};

pub fn transfer_tokens_with_hook_support<'info>(
    token_program: &Interface<'info, TokenInterface>,
    from: &InterfaceAccount<'info, TokenAccountInterface>,
    to: &InterfaceAccount<'info, TokenAccountInterface>,
    authority: &AccountInfo<'info>,
    mint: &InterfaceAccount<'info, MintInterface>,
    transfer_hook_program: Option<&UncheckedAccount<'info>>,
    amount: u64,
    signer_seeds: Option<&'info [&'info [&'info [u8]]]>,
) -> Result<()> {
    use anchor_spl::token_interface;
    use spl_token_2022::extension::transfer_hook::TransferHook;
    use spl_token_2022::extension::{ExtensionType, StateWithExtensions};

    let mut remaining_accounts: Vec<AccountInfo> = Vec::new();

    // Check if the mint is a Token-2022 mint and has a TransferHook extension
    let mint_info = mint.to_account_info();
    if mint_info.owner == &spl_token_2022::ID {
        if let Ok(mint_data_with_extensions) =
            StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_info.data.borrow())
        {
            if let Ok(_transfer_hook_extension) =
                mint_data_with_extensions.get_extension::<TransferHook>()
            {
                // If the mint has a transfer hook, ensure the hook program account is provided
                if let Some(hook_program_acc) = transfer_hook_program {
                    remaining_accounts.push(hook_program_acc.to_account_info());
                    // NOTE: If the specific transfer hook requires *other* accounts,
                    // they would also need to be added to `remaining_accounts` here.
                    // For a generic AMM, this is a common point of customization.
                } else {
                    // This case should ideally be caught by the `validate_transfer_hook_program` constraint
                    // but it's good to be explicit. Using a generic error since MissingTransferHookProgram
                    // might not exist in the error enum.
                    return Err(TradiumError::InvalidTransferHookProgram.into());
                }
            }
        }
    }

    let transfer_accounts = token_interface::Transfer {
        from: from.to_account_info(),
        to: to.to_account_info(),
        authority: authority.clone(),
    };

    // Create CPI context based on whether signer seeds are provided
    let transfer_ctx = if let Some(seeds) = signer_seeds {
        CpiContext::new_with_signer(token_program.to_account_info(), transfer_accounts, seeds)
    } else {
        CpiContext::new(token_program.to_account_info(), transfer_accounts)
    };

    // Add remaining_accounts to the CPI context
    let transfer_ctx = transfer_ctx.with_remaining_accounts(remaining_accounts);

    token_interface::transfer(transfer_ctx, amount)?;

    Ok(())
}

pub fn validate_transfer_hook_program(
    mint: &InterfaceAccount<MintInterface>,
    transfer_hook_program: &AccountInfo,
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
        // Regular SPL token but program was provided - invalid
        return false;
    }
}
