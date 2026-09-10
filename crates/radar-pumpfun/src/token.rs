// SPDX-License-Identifier: Apache-2.0
//! SPL Token and Token-2022 accounts, and the reserve one of them holds.
//!
//! [`Pool`](crate::Pool) names two token accounts and holds neither balance. This
//! module reads those accounts. It is the missing caller research 0033 §"What
//! this does not establish" recorded: *"the reserves are the balances of the two
//! token accounts this struct names, and no SPL or Token-2022 token-account
//! parser exists in this repository."*
//!
//! # Which program owns it is a fact about the account, not about the pool
//!
//! `Pool` has no `base_token_program` or `quote_token_program` field -- the 2026-09-09
//! capture settled that, because the documented field order accounts for every
//! byte and the rest is zero. So the program is read from the account's **owner**,
//! and it has to be, because it is not constant: of ten pools captured, five use
//! a different program on each side, and which side carries which varies. A
//! parser that assumed classic SPL would read a Token-2022 account's first 165
//! bytes correctly and then silently ignore everything that made it different.
//!
//! # Extensions are read, and most of them are refused
//!
//! Token-2022's extensions are not decoration. A transfer fee means the amount
//! that leaves a vault is not the amount that arrives. A transfer hook means an
//! arbitrary program runs on every move and may refuse it. A permanent delegate
//! means someone can take the reserve. `Pausable` means the whole mint can be
//! stopped. Each of those changes what a balance is worth or whether it can move
//! at all, and none of them is modelled here.
//!
//! So the rule is rule 9's: **unknown is not safe**. Three extensions are
//! accepted, because they were captured on PumpSwap's own vaults and mints and
//! because none of them can change a number:
//!
//! | extension | where it was captured | why it changes nothing |
//! |---|---|---|
//! | `ImmutableOwner` (7) | every Token-2022 vault of the four pools captured | the account's owner cannot be reassigned; the balance is untouched |
//! | `MetadataPointer` (18) | both Token-2022 mints captured | an address where a name and a symbol live |
//! | `TokenMetadata` (19) | both Token-2022 mints captured | the name and symbol themselves |
//!
//! Every other extension is **refused by name**, and an extension code this
//! module has never heard of is refused by number. That includes ones that are
//! probably harmless: refusing a mint for a `GroupPointer` costs a quote that
//! could have been given, and accepting one for a reason no capture supports
//! costs a price that is wrong. The list grows when a capture and an argument
//! arrive together, not before.
//!
//! # What the names are worth
//!
//! Nine extension codes here were **captured**: 1, 3, 4, 7, 12, 14, 16, 18 and
//! 19, each at exactly the byte length the program's layout gives it. The rest of
//! the table is the published `ExtensionType` order and is **not** captured --
//! offered as reasonable inference, in AGENTS.md §2's terms, and marked as such
//! on [`Extension::name`]. Getting an uncaptured name wrong mislabels a refusal;
//! it cannot turn one into an acceptance, because the refusal keys off the code
//! not being in the accepted list. That is why the uncaptured half is allowed to
//! be inference at all.
//!
//! # Raw, not effective
//!
//! [`Reserve`] is the vault's balance as the chain holds it. The venue's own rule
//! is `effective_quote_reserves = quote vault balance + virtual_quote_reserves`,
//! and research 0033 measured `virtual_quote_reserves` without establishing what
//! it means: two pools carried values 128 apart whose sizes are not alike, which
//! is not how a per-pool virtual reserve behaves. Until that is understood there
//! is no effective reserve to compute, so this module reports the raw balance and
//! says so rather than shipping a plausible number.

use core::fmt;

use radar_types::{Address, Asset};

/// The classic SPL Token program, `Tokenkeg…VQ5DA`.
pub const SPL_TOKEN_PROGRAM: Address = Address::new([
    0x06, 0xdd, 0xf6, 0xe1, 0xd7, 0x65, 0xa1, 0x93, 0xd9, 0xcb, 0xe1, 0x46, 0xce, 0xeb, 0x79, 0xac,
    0x1c, 0xb4, 0x85, 0xed, 0x5f, 0x5b, 0x37, 0x91, 0x3a, 0x8c, 0xf5, 0x85, 0x7e, 0xff, 0x00, 0xa9,
]);

/// The Token-2022 program, `TokenzQd…PxuEb`.
pub const TOKEN_2022_PROGRAM: Address = Address::new([
    0x06, 0xdd, 0xf6, 0xe1, 0xee, 0x75, 0x8f, 0xde, 0x18, 0x42, 0x5d, 0xbc, 0xe4, 0x6c, 0xcd, 0xda,
    0xb6, 0x1a, 0xfc, 0x4d, 0x83, 0xb9, 0x0d, 0x27, 0xfe, 0xbd, 0xf9, 0x28, 0xd8, 0xa1, 0x8b, 0xfc,
]);

/// A classic SPL token account, and the base of a Token-2022 one.
pub const TOKEN_ACCOUNT_LEN: usize = 165;
/// A classic SPL mint, and the base of a Token-2022 one.
pub const MINT_LEN: usize = 82;
/// Where Token-2022 puts the byte saying which kind of account this is.
///
/// The same offset for both kinds, which is why a Token-2022 mint is padded out
/// to [`TOKEN_ACCOUNT_LEN`] before it: the discriminating byte has to land in the
/// same place whatever it discriminates. Every captured mint holds zeroes across
/// that padding. Nothing here requires that -- it is a region the program does
/// not read, so refusing a non-zero byte in it would be a check on something that
/// cannot affect an answer.
pub const ACCOUNT_TYPE_AT: usize = 165;
/// Where the extension list starts, when there is one.
pub const EXTENSIONS_AT: usize = ACCOUNT_TYPE_AT + 1;

/// `AccountType::Mint`, the byte at [`ACCOUNT_TYPE_AT`] of an extended mint.
const ACCOUNT_TYPE_MINT: u8 = 1;
/// `AccountType::Account`, the byte at [`ACCOUNT_TYPE_AT`] of an extended token account.
const ACCOUNT_TYPE_ACCOUNT: u8 = 2;

/// Which of the two token programs owns an account.
///
/// Held apart rather than folded into a boolean because the two are not "the same
/// thing with a flag": one has extensions and the other cannot, and [`Asset`]
/// keys off which, so a mint address under the two programs is two different
/// assets.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TokenProgram {
    /// The classic SPL Token program. No extensions exist.
    Spl,
    /// Token-2022. Extensions may exist and are read.
    Token2022,
}

impl TokenProgram {
    /// Which program an owner address is, or `None` for anything else.
    ///
    /// `None` is what a caller must refuse on. An account owned by neither token
    /// program is not an empty token account; it is not a token account, and
    /// reading its first 165 bytes as one would produce a mint, an owner and a
    /// balance out of unrelated data.
    #[must_use]
    pub fn of(owner: &Address) -> Option<Self> {
        if *owner == SPL_TOKEN_PROGRAM {
            Some(Self::Spl)
        } else if *owner == TOKEN_2022_PROGRAM {
            Some(Self::Token2022)
        } else {
            None
        }
    }

    /// The program's address.
    #[must_use]
    pub const fn id(self) -> Address {
        match self {
            Self::Spl => SPL_TOKEN_PROGRAM,
            Self::Token2022 => TOKEN_2022_PROGRAM,
        }
    }

    /// The asset a mint under this program denominates.
    ///
    /// Routes through [`Asset::spl`] and [`Asset::token_2022`], which is what
    /// keeps wrapped SOL and USDC from acquiring a second, unequal spelling.
    #[must_use]
    pub fn asset(self, mint: Address) -> Asset {
        match self {
            Self::Spl => Asset::spl(mint),
            Self::Token2022 => Asset::token_2022(mint),
        }
    }
}

/// A Token-2022 extension, by its type code.
///
/// A code rather than an enum of variants, because the set is open: the program
/// gains extensions, and a parser that could not represent the one it had not
/// heard of would have to either drop it or fail to compile against the future.
/// Refusing by number is a complete answer; naming it is a convenience for
/// whoever reads the error.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Extension(pub u16);

impl Extension {
    /// `ImmutableOwner`. Captured on every Token-2022 pool vault.
    pub const IMMUTABLE_OWNER: Self = Self(7);
    /// `MetadataPointer`. Captured on both Token-2022 pool mints.
    pub const METADATA_POINTER: Self = Self(18);
    /// `TokenMetadata`. Captured on both Token-2022 pool mints.
    pub const TOKEN_METADATA: Self = Self(19);

    /// The extensions that may appear without changing what a balance is.
    ///
    /// Exactly the three found on PumpSwap's own accounts on 2026-09-09. See the
    /// module documentation for why the list is this short and what it costs.
    pub const ACCEPTED: [Self; 3] = [
        Self::IMMUTABLE_OWNER,
        Self::METADATA_POINTER,
        Self::TOKEN_METADATA,
    ];

    /// Whether this extension leaves a balance meaning what it says.
    #[must_use]
    pub fn is_accepted(self) -> bool {
        Self::ACCEPTED.contains(&self)
    }

    /// The program's name for this code, or `None` for one not in the table.
    ///
    /// Codes 1, 3, 4, 7, 12, 14, 16, 18 and 19 were **captured** on mainnet, each
    /// at the byte length its layout gives it. The others are the published
    /// `ExtensionType` order and are inference, not measurement -- see the module
    /// documentation for why that is tolerable here and nowhere near a balance.
    #[must_use]
    pub const fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            0 => "Uninitialized",
            1 => "TransferFeeConfig",
            2 => "TransferFeeAmount",
            3 => "MintCloseAuthority",
            4 => "ConfidentialTransferMint",
            5 => "ConfidentialTransferAccount",
            6 => "DefaultAccountState",
            7 => "ImmutableOwner",
            8 => "MemoTransfer",
            9 => "NonTransferable",
            10 => "InterestBearingConfig",
            11 => "CpiGuard",
            12 => "PermanentDelegate",
            13 => "NonTransferableAccount",
            14 => "TransferHook",
            15 => "TransferHookAccount",
            16 => "ConfidentialTransferFeeConfig",
            17 => "ConfidentialTransferFeeAmount",
            18 => "MetadataPointer",
            19 => "TokenMetadata",
            20 => "GroupPointer",
            21 => "TokenGroup",
            22 => "GroupMemberPointer",
            23 => "TokenGroupMember",
            24 => "ConfidentialMintBurn",
            25 => "ScaledUiAmount",
            26 => "Pausable",
            27 => "PausableAccount",
            _ => return None,
        })
    }
}

impl fmt::Display for Extension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => write!(f, "{name} ({})", self.0),
            None => write!(f, "extension {} (no name known)", self.0),
        }
    }
}

/// Whether a token account may move its balance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AccountState {
    /// Allocated but never initialised. Holds nothing.
    Uninitialized,
    /// Ordinary.
    Initialized,
    /// The mint's freeze authority has frozen it. Nothing moves in or out.
    Frozen,
}

/// Someone other than the owner who may move part of the balance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Delegate {
    /// Who may move it.
    pub who: Address,
    /// How much of the balance they may move.
    pub amount: u64,
}

/// A token account, under either program.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TokenAccount {
    /// Which program owns it.
    pub program: TokenProgram,
    /// The mint whose units this holds.
    pub mint: Address,
    /// Whose account it is. For a pool vault, the pool.
    pub owner: Address,
    /// The balance, in the mint's smallest unit. **Not** lamports unless the mint
    /// happens to be wrapped SOL, and six of ten captured pools quote in
    /// something else.
    pub amount: u64,
    /// Whether it is frozen.
    pub state: AccountState,
    /// A delegation, when one is set.
    pub delegate: Option<Delegate>,
}

/// A mint, under either program.
///
/// The mint account does not contain its own address, so nothing here is the
/// mint's identity -- the caller holds that, and [`Reserve::new`] is where the two
/// are checked against each other.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MintAccount {
    /// Which program owns it.
    pub program: TokenProgram,
    /// How many decimal places the smallest unit is. **Not a constant**: of the
    /// mints captured, wrapped SOL has 9, USDC has 6, a pump coin has 6 and one
    /// base mint has 7.
    pub decimals: u8,
    /// Total supply, in the smallest unit.
    pub supply: u64,
    /// Who may freeze accounts of this mint, when anyone may.
    ///
    /// `Some` is not a refusal: USDC has one and USDC is a legitimate quote
    /// asset. It is recorded because whether a position can be frozen shut is a
    /// fact an exit has to weigh, and that weighing is a later slice's.
    pub freeze_authority: Option<Address>,
    /// Whether the mint has been initialised.
    pub initialized: bool,
}

/// Why an account could not be read as a token account or a mint.
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
pub enum TokenMalformed {
    /// The account is owned by neither token program.
    #[error("owned by {owner}, which is neither token program")]
    NotATokenProgram {
        /// Who owns it.
        owner: Address,
    },
    /// A classic SPL account is not exactly the length its layout has.
    ///
    /// Exact, not "at least": the classic program has no extensions, so a longer
    /// account under it is something else -- a multisig is 355 bytes and would
    /// otherwise parse as a token account with a nonsense balance.
    #[error(
        "{len} bytes under the SPL Token program, which has only {expected}-byte accounts of this kind"
    )]
    WrongLength {
        /// How many bytes arrived.
        len: usize,
        /// The one length that program uses.
        expected: usize,
    },
    /// Fewer bytes than the base layout needs.
    #[error("{len} bytes, and the layout needs {needed}")]
    TooShort {
        /// How many arrived.
        len: usize,
        /// How many the layout needs.
        needed: usize,
    },
    /// The Token-2022 account-type byte says this is the other kind of account.
    ///
    /// A mint read as a token account would put the mint authority where the
    /// mint belongs and the supply where the balance belongs, and every value
    /// would look plausible.
    #[error("account type byte is {found}, and {expected} was expected")]
    WrongAccountType {
        /// What the byte said.
        found: u8,
        /// What this parse required.
        expected: u8,
    },
    /// The state byte is not one of the three states.
    #[error("account state byte is {found}, which names no state")]
    UnknownState {
        /// What the byte said.
        found: u8,
    },
    /// A four-byte `COption` tag is neither zero nor one.
    #[error("{field} option tag is {found}, which is neither none nor some")]
    NotACOption {
        /// Which optional field.
        field: &'static str,
        /// What the tag said.
        found: u32,
    },
    /// An extension header runs past the end of the account.
    #[error("{extension} at {at} claims {length} bytes, past the end of a {account}-byte account")]
    ExtensionOverruns {
        /// The extension whose header it is.
        extension: Extension,
        /// Where the header sits.
        at: usize,
        /// What it claimed.
        length: u16,
        /// How long the account is.
        account: usize,
    },
    /// Bytes remain where an extension header should start but cannot fit.
    #[error("{remaining} bytes at {at} are too few for an extension header")]
    TrailingExtensionBytes {
        /// Where they start.
        at: usize,
        /// How many there are.
        remaining: usize,
    },
    /// An extension is present whose effect on a balance is not modelled.
    ///
    /// The refusal names it. This is rule 9 at its sharpest: a transfer fee, a
    /// hook, a permanent delegate or a pause each make the number in the account
    /// mean something other than what it says, and reporting the number anyway
    /// would be a price nobody would question.
    #[error(
        "{extension} changes what this balance is worth or whether it can move, and is not modelled"
    )]
    UnmodelledExtension {
        /// Which one.
        extension: Extension,
    },
    /// The vault holds a different mint from the one the decimals came from.
    #[error("vault holds {held}, and the decimals were read from {read_from}")]
    MintMismatch {
        /// The mint the token account names.
        held: Address,
        /// The mint whose account supplied the decimals.
        read_from: Address,
    },
    /// The vault cannot move its balance, so it is not a reserve.
    #[error("vault is {state:?}, so its balance is not liquidity")]
    Immobile {
        /// Which state it is in.
        state: AccountState,
    },
    /// Someone other than the owner may move part of the balance.
    #[error("{amount} of the balance is delegated to {who}, who may move it")]
    Delegated {
        /// Who may move it.
        who: Address,
        /// How much.
        amount: u64,
    },
}

/// Reads a four-byte `COption` tag and the value behind it.
fn coption<T>(
    data: &[u8],
    tag_at: usize,
    field: &'static str,
    value: impl FnOnce() -> T,
) -> Result<Option<T>, TokenMalformed> {
    let tag = u32::from_le_bytes(
        data[tag_at..tag_at + 4]
            .try_into()
            .expect("checked by caller"),
    );
    match tag {
        0 => Ok(None),
        1 => Ok(Some(value())),
        found => Err(TokenMalformed::NotACOption { field, found }),
    }
}

/// Walks the Token-2022 extension list and refuses anything not modelled.
///
/// Stops at an `Uninitialized` (0) entry, which is how the program's own reader
/// treats spare allocation. That behaviour is taken from the program and is
/// **not** captured: no account seen on 2026-09-09 had spare room after its last
/// extension. It is safe in the direction that matters -- stopping early can only
/// end the walk, and the accounts that do carry an unmodelled extension carry it
/// as a real entry before any zero.
fn extensions(data: &[u8], expected_type: u8) -> Result<(), TokenMalformed> {
    let found = data[ACCOUNT_TYPE_AT];
    if found != expected_type {
        return Err(TokenMalformed::WrongAccountType {
            found,
            expected: expected_type,
        });
    }

    let mut at = EXTENSIONS_AT;
    loop {
        let remaining = data.len() - at;
        if remaining == 0 {
            return Ok(());
        }
        if remaining < 4 {
            return Err(TokenMalformed::TrailingExtensionBytes { at, remaining });
        }
        let extension = Extension(u16::from_le_bytes(
            data[at..at + 2].try_into().expect("four bytes remain"),
        ));
        let length =
            u16::from_le_bytes(data[at + 2..at + 4].try_into().expect("four bytes remain"));
        if extension.0 == 0 {
            return Ok(());
        }
        let end = at + 4 + usize::from(length);
        if end > data.len() {
            return Err(TokenMalformed::ExtensionOverruns {
                extension,
                at,
                length,
                account: data.len(),
            });
        }
        if !extension.is_accepted() {
            return Err(TokenMalformed::UnmodelledExtension { extension });
        }
        at = end;
    }
}

/// Which shapes a length is allowed to be, and whether extensions follow.
///
/// # The gap this closes
///
/// A Token-2022 mint is either exactly [`MINT_LEN`] with no extensions, or it is
/// padded out past [`ACCOUNT_TYPE_AT`] so that the account-type byte lands in the
/// same place a token account's does. Nothing legitimate sits between. Allowing
/// the between would let a **165-byte Token-2022 token account parse as a mint**:
/// it is longer than 82, it has no byte at [`ACCOUNT_TYPE_AT`] to check, and it
/// would yield the mint authority's option tag as `mint_authority`, part of the
/// owner as the supply, and one byte of it as the decimals. Every field would
/// look ordinary and the scale would be wrong by an arbitrary power of ten.
///
/// For a token account [`TOKEN_ACCOUNT_LEN`] and [`ACCOUNT_TYPE_AT`] are the same
/// number, so the middle band is empty and this reduces to "165, or more".
fn shape(program: TokenProgram, len: usize, base: usize) -> Result<bool, TokenMalformed> {
    if len == base {
        return Ok(false);
    }
    match program {
        // The classic program has no extensions at all, so a longer account is a
        // different kind of account -- a multisig is 355 bytes and would parse as
        // a token account with a nonsense balance.
        TokenProgram::Spl => Err(TokenMalformed::WrongLength {
            len,
            expected: base,
        }),
        TokenProgram::Token2022 if len > ACCOUNT_TYPE_AT => Ok(true),
        TokenProgram::Token2022 => Err(TokenMalformed::TooShort {
            len,
            needed: EXTENSIONS_AT,
        }),
    }
}

impl TokenAccount {
    /// Reads a token account, given the program that owns it.
    ///
    /// The owner is an argument because it is not in the data. It comes from the
    /// same RPC response the bytes did.
    ///
    /// # Errors
    ///
    /// [`TokenMalformed`]. In particular an owner that is neither token program,
    /// a Token-2022 extension whose effect is not modelled -- named in the error
    /// -- and a state byte outside the three states.
    ///
    /// # Panics
    ///
    /// Cannot. Every slice is inside a length checked by [`shape`] first.
    pub fn parse(data: &[u8], owner_program: &Address) -> Result<Self, TokenMalformed> {
        let program = TokenProgram::of(owner_program).ok_or(TokenMalformed::NotATokenProgram {
            owner: *owner_program,
        })?;
        let extended = shape(program, data.len(), TOKEN_ACCOUNT_LEN)?;
        // Before any field: what kind of account this is, and whether anything
        // about it is unmodelled. Reading fields first would let a mint be
        // refused for having an implausible delegate tag rather than for being a
        // mint, which is a true refusal that says the wrong thing.
        if extended {
            extensions(data, ACCOUNT_TYPE_ACCOUNT)?;
        }

        let address = |at: usize| -> Address {
            Address::new(data[at..at + 32].try_into().expect("length checked"))
        };
        let state = match data[108] {
            0 => AccountState::Uninitialized,
            1 => AccountState::Initialized,
            2 => AccountState::Frozen,
            found => return Err(TokenMalformed::UnknownState { found }),
        };
        let delegate = coption(data, 72, "delegate", || Delegate {
            who: address(76),
            amount: u64::from_le_bytes(data[121..129].try_into().expect("length checked")),
        })?;
        // Read and refused rather than read and kept: the tag has to be one of
        // the two, and a third value means these are not the bytes this layout
        // describes. `close_authority` cannot remove a reserve on its own --
        // both programs refuse to close an account with a balance -- so the
        // address behind the tag is not carried.
        coption(data, 129, "close_authority", || ())?;
        // Likewise `is_native`, which distinguishes a wrapped-SOL account. It
        // changes nothing about `amount`.
        coption(data, 109, "is_native", || ())?;

        Ok(Self {
            program,
            mint: address(0),
            owner: address(32),
            amount: u64::from_le_bytes(data[64..72].try_into().expect("length checked")),
            state,
            delegate,
        })
    }
}

impl MintAccount {
    /// Reads a mint, given the program that owns it.
    ///
    /// # Errors
    ///
    /// [`TokenMalformed`], on the same terms as [`TokenAccount::parse`].
    ///
    /// # Panics
    ///
    /// Cannot. Every slice is inside a length checked by [`shape`] first.
    pub fn parse(data: &[u8], owner_program: &Address) -> Result<Self, TokenMalformed> {
        let program = TokenProgram::of(owner_program).ok_or(TokenMalformed::NotATokenProgram {
            owner: *owner_program,
        })?;
        let extended = shape(program, data.len(), MINT_LEN)?;
        // Before any field, for the reason given in `TokenAccount::parse`.
        if extended {
            extensions(data, ACCOUNT_TYPE_MINT)?;
        }

        coption(data, 0, "mint_authority", || ())?;
        let freeze_authority = coption(data, 46, "freeze_authority", || {
            Address::new(data[50..82].try_into().expect("length checked"))
        })?;

        Ok(Self {
            program,
            decimals: data[44],
            supply: u64::from_le_bytes(data[36..44].try_into().expect("length checked")),
            freeze_authority,
            initialized: data[45] != 0,
        })
    }
}

/// One side of a pool's liquidity: a balance that knows what it counts.
///
/// # Why this is not an integer
///
/// A `u64` off a vault is three facts short of a quantity. It does not say which
/// mint it counts, and six of the ten pools captured on 2026-09-09 quote in
/// something other than SOL -- one of them in USDC. It does not say how many
/// decimals that mint has, and the captured mints have 6, 7 and 9. And it does
/// not say which token program the mint lives under, which is what decides
/// whether a transfer of it is plain or subject to an extension.
///
/// # Raw, and why there is no effective reserve here
///
/// [`raw`](Self::raw) is the balance the vault holds. The venue's own rule for a
/// quote is `effective_quote_reserves = quote vault balance + virtual_quote_reserves`,
/// and research 0033 read that second term without establishing its meaning --
/// two pools carried values 128 apart, which is not how a per-pool virtual
/// reserve behaves. There is therefore no effective reserve to compute, and this
/// type deliberately offers none rather than one that would be believed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Reserve {
    /// Which asset, keyed by mint and by which program the mint lives under.
    pub asset: Asset,
    /// The mint's decimal places. Carried alongside because [`Asset`] identifies
    /// a mint and does not scale it.
    pub decimals: u8,
    /// The vault balance, in the mint's smallest unit. Raw, not effective.
    pub raw: u64,
    /// The token account this was read from, so a figure can be checked on an
    /// explorer.
    pub vault: Address,
}

impl Reserve {
    /// Builds a reserve from a vault and the mint that gives it a scale.
    ///
    /// This is the only constructor, and it is where three things are made
    /// impossible rather than merely discouraged: decimals from a different mint
    /// than the vault holds, a balance in a frozen or uninitialised account, and
    /// a balance part of which someone else may move.
    ///
    /// # Errors
    ///
    /// - [`MintMismatch`](TokenMalformed::MintMismatch) when the vault's mint is
    ///   not the mint the decimals came from. Scaling a balance by another mint's
    ///   decimals is wrong by a power of ten and looks entirely reasonable.
    /// - [`Immobile`](TokenMalformed::Immobile) for a frozen or uninitialised
    ///   vault. A balance that cannot move is not liquidity, and rule 9 says an
    ///   unmovable reserve is a refusal rather than a number.
    /// - [`Delegated`](TokenMalformed::Delegated) when part of the balance is
    ///   delegated away. The delegate may take it between the read and a trade.
    pub fn new(
        vault: Address,
        account: &TokenAccount,
        mint_address: Address,
        mint: &MintAccount,
    ) -> Result<Self, TokenMalformed> {
        if account.mint != mint_address {
            return Err(TokenMalformed::MintMismatch {
                held: account.mint,
                read_from: mint_address,
            });
        }
        if account.state != AccountState::Initialized {
            return Err(TokenMalformed::Immobile {
                state: account.state,
            });
        }
        if let Some(delegate) = account.delegate
            && delegate.amount > 0
        {
            return Err(TokenMalformed::Delegated {
                who: delegate.who,
                amount: delegate.amount,
            });
        }
        Ok(Self {
            asset: mint.program.asset(mint_address),
            decimals: mint.decimals,
            raw: account.amount,
            vault,
        })
    }
}
