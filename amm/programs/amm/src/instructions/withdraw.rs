use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        burn_checked, transfer_checked, BurnChecked, Mint, TokenAccount, TokenInterface,
        TransferChecked,
    },
};
use constant_product_curve::{ConstantProduct, XYAmounts};

use crate::{error::AmmError, state::Config};

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub withdrawer: Signer<'info>,

    pub mint_x: Box<InterfaceAccount<'info, Mint>>,
    pub mint_y: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        has_one = mint_x,
        has_one = mint_y,
        seeds =[b"config", config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump,
    )]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = config,
        associated_token::token_program = token_program,
    )]
    pub vault_x: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = config,
        associated_token::token_program = token_program,
    )]
    pub vault_y: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"lp", config.key().as_ref()],
        bump = config.lp_bump,
    )]
    pub mint_lp: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = mint_lp,
        associated_token::authority = withdrawer,
        associated_token::token_program = token_program
    )]
    pub user_lp: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = withdrawer,
        associated_token::mint = mint_x,
        associated_token::authority = withdrawer,
        associated_token::token_program = token_program
    )]
    pub user_x: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = withdrawer,
        associated_token::mint = mint_y,
        associated_token::authority = withdrawer,
        associated_token::token_program = token_program
    )]
    pub user_y: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> Withdraw<'info> {
    pub fn withdraw(&mut self, amount: u64, min_x: u64, min_y: u64) -> Result<()> {
        require!(!self.config.locked, AmmError::PoolLocked);
        require!(amount != 0, AmmError::ZeroBalance);
        require!(min_x != 0 || min_y != 0, AmmError::InvalidAmount);
        require!(amount <= self.user_lp.amount, AmmError::InsufficientBalance);

        let XYAmounts {
            x: amount_x,
            y: amount_y,
        } = ConstantProduct::xy_withdraw_amounts_from_l(
            self.vault_x.amount,
            self.vault_y.amount,
            self.mint_lp.supply,
            amount,
            6,
        )
        .map_err(AmmError::from)?;

        require!(
            min_x <= amount_x && min_y <= amount_y,
            AmmError::SlippageExceeded
        );

        self.withdraw_tokens(true, amount_x)?;
        self.withdraw_tokens(false, amount_y)?;

        self.burn_lp_tokens(amount)?;
        Ok(())
    }

    pub fn withdraw_tokens(&self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to, mint, mint_decimals) = match is_x {
            true => (
                self.vault_x.to_account_info(),
                self.user_x.to_account_info(),
                self.mint_x.to_account_info(),
                self.mint_x.decimals,
            ),
            false => (
                self.vault_y.to_account_info(),
                self.user_y.to_account_info(),
                self.mint_y.to_account_info(),
                self.mint_y.decimals,
            ),
        };

        let transfer_account = TransferChecked {
            from: from,
            mint: mint,
            to: to,
            authority: self.config.to_account_info(),
        };

        let config_seeds = self.config.seed.to_le_bytes();
        let signer_seeds: &[&[&[u8]]] =
            &[&[b"config", config_seeds.as_ref(), &[self.config.config_bump]]];

        transfer_checked(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                transfer_account,
                signer_seeds,
            ),
            amount,
            mint_decimals,
        )?;

        Ok(())
    }
    pub fn burn_lp_tokens(&self, amount: u64) -> Result<()> {
        burn_checked(
            CpiContext::new(
                self.token_program.to_account_info(),
                BurnChecked {
                    mint: self.mint_lp.to_account_info(),
                    from: self.user_lp.to_account_info(),
                    authority: self.withdrawer.to_account_info(),
                },
            ),
            amount,
            self.mint_lp.decimals,
        )?;
        Ok(())
    }
}
