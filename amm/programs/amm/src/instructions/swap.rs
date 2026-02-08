use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
};
use constant_product_curve::{ConstantProduct, LiquidityPair};

use crate::{error::AmmError, state::Config};

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    pub mint_x: Box<InterfaceAccount<'info, Mint>>,
    pub mint_y: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        mut,
        seeds = [b"config", config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump,
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [b"lp", config.key().as_ref()],
        bump = config.lp_bump,
    )]
    pub mint_lp: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = config,
        associated_token::token_program = token_program
    )]
    pub vault_x: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = config,
        associated_token::token_program = token_program
    )]
    pub vault_y: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_x: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint_y,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_y: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> Swap<'info> {
    pub fn swap(&mut self, is_x: bool, amount: u64, min: u64) -> Result<()> {
        let (to_vault_amount, from_vault_amount) = match is_x {
            true => (self.vault_x.amount, self.vault_y.amount),
            false => (self.vault_y.amount, self.vault_x.amount),
        };

        let mut curve = ConstantProduct::init(
            self.vault_x.amount,
            self.vault_y.amount,
            self.mint_lp.supply,
            self.config.fee,
            None,
        )
        .unwrap();

        let liquidity_pair = match is_x {
            true => LiquidityPair::X,
            false => LiquidityPair::Y,
        };

        let swap_result = curve.swap(liquidity_pair, amount, min).unwrap();

        require_neq!(swap_result.deposit, 0, AmmError::InvalidAmount);
        require_neq!(swap_result.withdraw, 0, AmmError::InvalidAmount);

        self.deposit_tokens(is_x, swap_result.deposit)?;

        self.withdraw_tokens(is_x, swap_result.withdraw)?;

        Ok(())
    }

    pub fn deposit_tokens(&mut self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to, mint) = match is_x {
            true => (&mut self.user_x, &mut self.vault_x, &self.mint_x),
            false => (&mut self.user_y, &mut self.vault_y, &self.mint_y),
        };

        transfer_checked(
            CpiContext::new(
                self.token_program.to_account_info(),
                TransferChecked {
                    authority: self.user.to_account_info(),
                    from: from.to_account_info(),
                    to: to.to_account_info(),
                    mint: mint.to_account_info(),
                },
            ),
            amount,
            mint.decimals,
        )?;
        Ok(())
    }

    pub fn withdraw_tokens(&mut self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to, mint) = match is_x {
            true => (&mut self.vault_y, &mut self.user_y, &self.mint_y),
            false => (&mut self.vault_x, &mut self.user_x, &self.mint_x),
        };

        let signer_seeds: &[&[&[u8]]] = &[&[
            b"config",
            &self.config.seed.to_le_bytes(),
            &[self.config.config_bump],
        ]];

        transfer_checked(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                TransferChecked {
                    authority: self.config.to_account_info(),
                    from: from.to_account_info(),
                    to: to.to_account_info(),
                    mint: mint.to_account_info(),
                },
                signer_seeds,
            ),
            amount,
            mint.decimals,
        )?;
        Ok(())
    }
}
