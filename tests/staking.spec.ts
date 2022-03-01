import * as spl from '@solana/spl-token';
import * as anchor from '@project-serum/anchor';
import { Program } from '@project-serum/anchor';
import { expect } from 'chai';
import { Staking } from '../target/types/staking';

const { web3 } = anchor; 
const { Keypair, PublicKey } = anchor.web3;
const BN = anchor.BN;

function bn(value, decimals = 9) {
  return new BN(value).mul(new BN(10).pow(new BN(decimals)));
}

async function pda(programId, seeds) {
  for (let i = 0; i < seeds.length; i++) {
    if (typeof seeds[i] == 'string') {
      seeds[i] = Buffer.from(seeds[i]);
    }
    if (typeof seeds[i].toBuffer == 'function') {
      seeds[i] = seeds[i].toBuffer();
    }
  }
  return await PublicKey.findProgramAddress(seeds, programId);
}

describe('staking', () => {
  anchor.setProvider(anchor.Provider.env());
  const program = anchor.workspace.Staking as Program<Staking>;
  const programId = program.programId;
  const wallet = program.provider.wallet;

  it('works', async () => {
    const tokenNftAuthority = Keypair.generate();
    const tokenNftMint = await spl.Token.createMint(
      program.provider.connection,
      wallet.payer,
      tokenNftAuthority.publicKey,
      null,
      9,
      spl.TOKEN_PROGRAM_ID
    );
    const tokenNftUser = await tokenNftMint.createAccount(wallet.publicKey);
    await tokenNftMint.mintTo(
      tokenNftUser,
      tokenNftAuthority,
      [],
      bn(10, 9).toString()
    );

    const tokenRewardsAuthority = Keypair.generate();
    const tokenRewardsMint = await spl.Token.createMint(
      program.provider.connection,
      wallet.payer,
      tokenRewardsAuthority.publicKey,
      null,
      9,
      spl.TOKEN_PROGRAM_ID
    );
    const tokenRewardsUser = await tokenRewardsMint.createAccount(wallet.publicKey);
    
    const stakingTmpKey = Keypair.generate().publicKey;
    const [stakingKey, stakingBump] = await pda(programId, ["staking", stakingTmpKey]);
    const [stakingTokenKey, stakingTokenBump] = await pda(programId, ["staking-token", stakingTmpKey]);
    const [stakingTokenRewardsKey, stakingTokenRewardsBump] = await pda(programId, ["staking-token-rewards", stakingTmpKey]);

    await program.rpc.initialize(new BN(1), new BN(173611), stakingTmpKey, stakingBump, stakingTokenBump, stakingTokenRewardsBump, {
      accounts: {
        signer: wallet.publicKey,
        staking: stakingKey,
        tokenMint: tokenNftMint.publicKey,
        tokenRewardsMint: tokenRewardsMint.publicKey,
        tokenVault: stakingTokenKey,
        tokenRewardsVault: stakingTokenRewardsKey,
        rent: web3.SYSVAR_RENT_PUBKEY,
        tokenProgram: spl.TOKEN_PROGRAM_ID,
        systemProgram: web3.SystemProgram.programId,
      },
    });

    let stakingData = await program.account.staking.fetch(stakingKey);
    console.log('staking', stakingData);
    expect(stakingData.cap.toString()).to.equal('1');
    expect(stakingData.rate.toString()).to.equal('173611');

    await program.rpc.configure(new BN(2), new BN(173612), {
      accounts: {
        signer: wallet.publicKey,
        staking: stakingKey,
      },
    });
    stakingData = await program.account.staking.fetch(stakingKey);
    expect(stakingData.cap.toString()).to.equal('2');
    expect(stakingData.rate.toString()).to.equal('173612');

    let newSignerKey = Keypair.generate().publicKey;
    await program.rpc.configureSigner({
      accounts: {
        signer: wallet.publicKey,
        staking: stakingKey,
        newSigner: newSignerKey,
      },
    });
    stakingData = await program.account.staking.fetch(stakingKey);
    expect(stakingData.signer.toString()).to.equal(newSignerKey.toString());

    // Give staking contract rewards to hand out
    await tokenRewardsMint.mintTo(
      stakingData.tokenRewardsVault,
      tokenRewardsAuthority,
      [],
      bn(100000, 9).toString()
    );

    // Setup first user account
    const [stakingUserKey, stakingUserBump] = await pda(programId, ["staking-user", stakingTmpKey, wallet.publicKey]);
    await program.rpc.initializeUser(stakingUserBump, {
      accounts: {
        signer: wallet.publicKey,
        staking: stakingKey,
        user: stakingUserKey,
        systemProgram: web3.SystemProgram.programId,
      },
    });
    let stakingUserData = await program.account.stakingUser.fetch(stakingUserKey);
    console.log('user', stakingUserData);
    expect(stakingUserData.amount.toString()).to.equal('0');

    // Deposit tokens
    await program.rpc.deposit(new BN(1), {
      accounts: {
        signer: wallet.publicKey,
        staking: stakingKey,
        user: stakingUserKey,
        tokenUser: tokenNftUser,
        tokenVault: stakingTokenKey,
        tokenRewardsUser: tokenRewardsUser,
        tokenRewardsVault: stakingTokenRewardsKey,
        tokenProgram: spl.TOKEN_PROGRAM_ID,
      },
    });

    stakingUserData = await program.account.stakingUser.fetch(stakingUserKey);
    console.log('deposit', stakingUserData);
    expect(stakingUserData.amount.toString()).to.equal('1');

    await program.rpc.withdraw(new BN(1), {
      accounts: {
        signer: wallet.publicKey,
        staking: stakingKey,
        user: stakingUserKey,
        tokenUser: tokenNftUser,
        tokenVault: stakingTokenKey,
        tokenRewardsUser: tokenRewardsUser,
        tokenRewardsVault: stakingTokenRewardsKey,
        tokenProgram: spl.TOKEN_PROGRAM_ID,
      },
    });
    stakingUserData = await program.account.stakingUser.fetch(stakingUserKey);
    expect(stakingUserData.amount.toString()).to.equal('0');

    const tokenNftUserData = await tokenNftMint.getAccountInfo(tokenNftUser);
    expect(tokenNftUserData.amount.toString()).to.equal('10000000000');
    const tokenRewardsUserData = await tokenRewardsMint.getAccountInfo(tokenRewardsUser);
    expect(tokenRewardsUserData.amount.toString()).to.equal('173612');
  });
});
