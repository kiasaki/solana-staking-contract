#![allow(clippy::integer_arithmetic)]

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

declare_id!("BKDN5ZyBsuC7EBCtvPfgGsvTAuHSXDE5Sh5V8HskQGU8");

#[error]
pub enum ErrorCode {
    #[msg("unauthorized")]
    Unauthorized,
    #[msg("overflow")]
    Overflow,
    #[msg("underflow")]
    Underflow,
    #[msg("not enough")]
    NotEnough,
    #[msg("over cap")]
    OverCap,
}

#[account]
#[derive(Default)]
pub struct Staking {
    pub bump: u8,
    pub key: Pubkey,
    pub signer: Pubkey,
    pub token_mint: Pubkey,
    pub token_vault: Pubkey,
    pub token_rewards_mint: Pubkey,
    pub token_rewards_vault: Pubkey,
    pub cap: u64,
    pub rate: u64,
}

#[account]
#[derive(Default)]
pub struct StakingUser {
    pub signer: Pubkey,
    pub time_last: i64,
    pub time_start: i64,
    pub amount: u64,
}

#[program]
pub mod staking {
    use super::*;

    #[derive(Accounts)]
    #[instruction(cap: u64, rate: u64, key: Pubkey, bump: u8, token_vault_bump: u8, token_rewards_vault_bump: u8)]
    pub struct Initialize<'info> {
        #[account(mut)]
        pub signer: Signer<'info>,
        #[account(
            init,
            payer = signer,
            seeds = [b"staking", key.as_ref()],
            bump = bump
        )]
        pub staking: Account<'info, Staking>,
        pub token_mint: Account<'info, Mint>,
        pub token_rewards_mint: Account<'info, Mint>,
        #[account(
            init,
            payer = signer,
            seeds = [b"staking-token", key.as_ref()],
            bump = token_vault_bump,
            token::mint = token_mint,
            token::authority = staking,
        )]
        pub token_vault: Account<'info, TokenAccount>,
        #[account(
            init,
            payer = signer,
            seeds = [b"staking-token-rewards", key.as_ref()],
            bump = token_rewards_vault_bump,
            token::mint = token_rewards_mint,
            token::authority = staking,
        )]
        pub token_rewards_vault: Account<'info, TokenAccount>,
        pub rent: Sysvar<'info, Rent>,
        pub token_program: Program<'info, Token>,
        pub system_program: Program<'info, System>,
    }

    pub fn initialize(
        ctx: Context<Initialize>,
        cap: u64,
        rate: u64,
        key: Pubkey,
        bump: u8,
        _token_vault_bump: u8,
        _token_rewards_vault_bump: u8,
    ) -> ProgramResult {
        let staking = &mut ctx.accounts.staking;
        staking.bump = bump;
        staking.key = key;
        staking.signer = ctx.accounts.signer.key();
        staking.token_mint = ctx.accounts.token_mint.key();
        staking.token_vault = ctx.accounts.token_vault.key();
        staking.token_rewards_mint = ctx.accounts.token_rewards_mint.key();
        staking.token_rewards_vault = ctx.accounts.token_rewards_vault.key();
        staking.cap = cap;
        staking.rate = rate;
        Ok(())
    }

    #[derive(Accounts)]
    pub struct Configure<'info> {
        pub signer: Signer<'info>,
        #[account(mut, has_one = signer @ ErrorCode::Unauthorized)]
        pub staking: Account<'info, Staking>,
    }

    pub fn configure(ctx: Context<Configure>, cap: u64, rate: u64) -> ProgramResult {
        ctx.accounts.staking.cap = cap;
        ctx.accounts.staking.rate = rate;
        Ok(())
    }

    #[derive(Accounts)]
    pub struct ConfigureSigner<'info> {
        pub signer: Signer<'info>,
        #[account(mut, has_one = signer @ ErrorCode::Unauthorized)]
        pub staking: Account<'info, Staking>,
        pub new_signer: UncheckedAccount<'info>,
    }

    pub fn configure_signer(ctx: Context<ConfigureSigner>) -> ProgramResult {
        ctx.accounts.staking.signer = ctx.accounts.new_signer.key();
        Ok(())
    }

    #[derive(Accounts)]
    #[instruction(bump: u8)]
    pub struct InitializeUser<'info> {
        #[account(mut)]
        pub signer: Signer<'info>,
        pub staking: Account<'info, Staking>,
        #[account(
            init,
            payer = signer,
            seeds = [b"staking-user", staking.key.as_ref(), signer.key().as_ref()],
            bump = bump,
        )]
        pub user: Account<'info, StakingUser>,
        pub system_program: Program<'info, System>,
    }

    pub fn initialize_user(ctx: Context<InitializeUser>, _bump: u8) -> ProgramResult {
        let user = &mut ctx.accounts.user;
        user.signer = ctx.accounts.signer.key();
        Ok(())
    }

    #[derive(Accounts)]
    pub struct Deposit<'info> {
        pub signer: Signer<'info>,
        pub staking: Account<'info, Staking>,
        #[account(mut, has_one = signer)]
        pub user: Account<'info, StakingUser>,
        #[account(
            mut,
            constraint = token_user.mint == staking.token_mint,
            constraint = token_user.owner == signer.key(),
        )]
        pub token_user: Box<Account<'info, TokenAccount>>,
        #[account(
            mut,
            constraint = token_vault.key() == staking.token_vault,
        )]
        pub token_vault: Box<Account<'info, TokenAccount>>,
        #[account(
            mut,
            constraint = token_rewards_user.mint == staking.token_rewards_mint,
            constraint = token_rewards_user.owner == signer.key(),
        )]
        pub token_rewards_user: Box<Account<'info, TokenAccount>>,
        #[account(
            mut,
            constraint = token_rewards_vault.key() == staking.token_rewards_vault,
        )]
        pub token_rewards_vault: Box<Account<'info, TokenAccount>>,
        pub token_program: Program<'info, Token>,
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> ProgramResult {
        let now = Clock::get()?.unix_timestamp;
        let staking = &ctx.accounts.staking;
        let user = &mut ctx.accounts.user;

        // Send pending rewards as we're about to update amount & time_last
        let passed = now
            .checked_sub(user.time_last)
            .ok_or(ErrorCode::Underflow)? as u64;
        let rewards = user
            .amount
            .checked_mul(passed)
            .ok_or(ErrorCode::Overflow)?
            .checked_mul(staking.rate)
            .ok_or(ErrorCode::Overflow)?;
        user.time_last = now;
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.token_rewards_vault.to_account_info(),
                    to: ctx.accounts.token_rewards_user.to_account_info(),
                    authority: ctx.accounts.staking.to_account_info(),
                },
                &[&[b"staking", staking.key.as_ref(), &[staking.bump]]],
            ),
            rewards,
        )?;

        // Transfer tokens in & update amount
        user.time_start = now;
        user.amount = user.amount.checked_add(amount).ok_or(ErrorCode::Overflow)?;
        require!(user.amount <= staking.cap, ErrorCode::OverCap);
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.token_user.to_account_info(),
                    to: ctx.accounts.token_vault.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                },
            ),
            amount,
        )?;

        Ok(())
    }

    #[derive(Accounts)]
    pub struct Withdraw<'info> {
        pub signer: Signer<'info>,
        pub staking: Account<'info, Staking>,
        #[account(mut, has_one = signer)]
        pub user: Account<'info, StakingUser>,
        #[account(
            mut,
            constraint = token_user.owner == signer.key(),
            constraint = token_user.mint == staking.token_mint,
        )]
        pub token_user: Box<Account<'info, TokenAccount>>,
        #[account(
            mut,
            constraint = token_vault.key() == staking.token_vault,
            constraint = token_vault.owner == staking.key(),
            constraint = token_vault.mint == staking.token_mint,
        )]
        pub token_vault: Box<Account<'info, TokenAccount>>,
        #[account(
            mut,
            constraint = token_rewards_user.owner == signer.key(),
            constraint = token_rewards_user.mint == staking.token_rewards_mint,
        )]
        pub token_rewards_user: Box<Account<'info, TokenAccount>>,
        #[account(
            mut,
            constraint = token_rewards_vault.key() == staking.token_rewards_vault,
            constraint = token_rewards_vault.owner == staking.key(),
            constraint = token_rewards_vault.mint == staking.token_rewards_mint,
        )]
        pub token_rewards_vault: Box<Account<'info, TokenAccount>>,
        pub token_program: Program<'info, Token>,
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> ProgramResult {
        let now = Clock::get()?.unix_timestamp;
        let staking = &ctx.accounts.staking;
        let user = &mut ctx.accounts.user;

        // Send pending rewards as we're about to update amount & time_last
        let passed = now
            .checked_sub(user.time_last)
            .ok_or(ErrorCode::Underflow)? as u64;
        let rewards = user
            .amount
            .checked_mul(passed)
            .ok_or(ErrorCode::Overflow)?
            .checked_mul(staking.rate)
            .ok_or(ErrorCode::Overflow)?;
        user.time_last = now;
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.token_rewards_vault.to_account_info(),
                    to: ctx.accounts.token_rewards_user.to_account_info(),
                    authority: ctx.accounts.staking.to_account_info(),
                },
                &[&[b"staking", staking.key.as_ref(), &[staking.bump]]],
            ),
            rewards,
        )?;

        // Transfer tokens out & update amount
        require!(amount <= user.amount, ErrorCode::NotEnough);
        user.amount -= amount;
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.token_vault.to_account_info(),
                    to: ctx.accounts.token_user.to_account_info(),
                    authority: ctx.accounts.staking.to_account_info(),
                },
                &[&[b"staking", staking.key.as_ref(), &[staking.bump]]],
            ),
            amount,
        )?;

        Ok(())
    }
}
