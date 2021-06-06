// This file is part of Setheum.

// Copyright (C) 2020-2021 Setheum Labs.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! # SERP Treasury Module
//!
//! ## Overview
//!
//! SERP Treasury manages the accumulated fees and bad standards generated by
//! Settmint, and handle excess serplus or standards timely in order to keep the
//! system healthy. It's the only entry for issuing/burning stable
//! coins for the entire system.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]

use frame_support::{pallet_prelude::*, transactional};
use frame_system::pallet_prelude::*;
use orml_traits::{MultiCurrency, MultiCurrencyExtended};
use primitives::{Amount, Balance, BlockNumber, CurrencyId};
use sp_runtime::{
	traits::{AccountIdConversion, One, Zero},
	DispatchError, DispatchResult, FixedPointNumber, ModuleId,
};
use support::{SerpAuction, SerpTreasury, SerpTreasuryExtended, DEXManager, Ratio};

mod benchmarking;
mod mock;
mod tests;
pub mod weights;

pub use module::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The origin which may update parameters and handle
		/// serplus/standard/reserve. Root can always do this.
		type UpdateOrigin: EnsureOrigin<Self::Origin>;

		/// The Currency for managing assets related to Settmint
		type Currency: MultiCurrencyExtended<Self::AccountId, CurrencyId = CurrencyId, Balance = Balance>;

		#[pallet::constant]
		/// Stablecoin currency id
		type GetStableCurrencyId: Get<CurrencyId>;

		#[pallet::constant]
		/// Setter (SETT) currency Stablecoin currency id
		type GetSetterCurrencyId: Get<CurrencyId>;

		/// SERP-TES Adjustment Frequency.
		/// Schedule for when to trigger SERP-TES
		/// (Blocktime/BlockNumber - every blabla block)
		type SerpTesSchedule: Get<BlockNumber>;

		/// SerpUp ratio for Serplus Auctions / Swaps
		type SerplusSerpupRatio: Get<Rate>;

		/// SerpUp ratio for SettPay Cashdrops
		type SettPaySerpupRatio: Get<Rate>;

		/// SerpUp ratio for Setheum Treasury
		type SetheumTreasurySerpupRatio: Get<Rate>;

		/// SerpUp ratio for Setheum Foundation's Charity Fund
		type CharityFundSerpupRatio: Get<Rate>;

		/// SerpUp ratio for Setheum Investment Fund (SIF) DAO
		type SIFSerpupRatio: Get<Rate>;

		#[pallet::constant]
		/// SerpUp pool/account for receiving funds SettPay Cashdrops
		/// SettPayTreasury account.
		type SettPayTreasuryAcc: Get<ModuleId>;

		#[pallet::constant]
		/// SerpUp pool/account for receiving funds Setheum Treasury
		/// SetheumTreasury account.
		type SetheumTreasuryAcc: Get<ModuleId>;

		#[pallet::constant]
		/// SerpUp pool/account for receiving funds Setheum Investment Fund (SIF) DAO
		/// SIF account.
		type SIFAcc: Get<ModuleId>;

		/// SerpUp pool/account for receiving funds Setheum Foundation's Charity Fund
		/// CharityFund account.
		type CharityFundAcc: Get<AccountId>;


		/// Auction manager creates different types of auction to handle system serplus and standard.
		type SerpAuctionHandler: SerpAuction<Self::AccountId, CurrencyId = CurrencyId, Balance = Balance>;

		/// Dex manager is used to swap reserve asset (Setter) for propper (SettCurrency).
		type DEX: DEXManager<Self::AccountId, CurrencyId, Balance>;

		#[pallet::constant]
		/// The cap of lots when an auction is created
		type MaxAuctionsCount: Get<u32>;

		#[pallet::constant]
		/// The SERP Treasury's module id, keeps serplus and reserve asset.
		type ModuleId: Get<ModuleId>;

		/// Weight information for the extrinsics in this module.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The setter amount of SERP Treasury is not enough
		SetterNotEnough,
		/// The serplus pool of SERP Treasury is not enough
		SerplusPoolNotEnough,
		/// Serplus pool overflow
		SerplusPoolOverflow,
		/// SettPay pool overflow
		SettPayTreasuryPoolOverflow,
		/// SetheumTreasury pool overflow
		SetheumTreasuryPoolOverflow,
		/// CharityFund pool overflow
		CharityFundPoolOverflow,
		/// SIF pool overflow
		SIFPoolOverflow,
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	pub enum Event<T: Config> {
		/// The expected amount size for per lot collateral auction of the
		/// reserve type updated. \[reserve_type, new_size\]
		ExpectedSetterAuctionSizeUpdated(CurrencyId, Balance),
		/// Currency SerpUp has been delivered successfully.
		CurrencySerpUpDelivered(Balance, CurrencyId),
		/// Currency SerpUp has been completed successfully.
		CurrencySerpedUp(Balance, CurrencyId),
		/// Currency SerpDown has been triggered successfully.
		CurrencySerpDownTriggered(Balance, CurrencyId),
		/// The Stablecoin Price is stable and indifferent from peg
		PriceIsStable(CurrencyId, Price),
	}

	/// The maximum amount of reserve amount for sale per setter auction
	#[pallet::storage]
	#[pallet::getter(fn expected_setter_auction_size)]
	pub type ExpectedSetterAuctionSize<T: Config> = StorageMap<_, Twox64Concat, CurrencyId, Balance, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub expected_setter_auction_size: Vec<(CurrencyId, Balance)>,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			GenesisConfig {
				expected_setter_auction_size: vec![],
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			self.expected_setter_auction_size
				.iter()
				.for_each(|(currency_id, size)| {
					ExpectedSetterAuctionSize::<T>::insert(currency_id, size);
				});
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {
		///
		/// NOTE: This function is called BEFORE ANY extrinsic in a block is applied,
		/// including inherent extrinsics. Hence for instance, if you runtime includes
		/// `pallet_timestamp`, the `timestamp` is not yet up to date at this point.
		/// Handle excessive surplus or debits of system when block end
		///
		/// Triggers Serping for all system stablecoins at every block.
		fn on_initialize(_now: T::BlockNumber) {
			/// SERP-TES Adjustment Frequency.
			/// Schedule for when to trigger SERP-TES
			/// (Blocktime/BlockNumber - every blabla block)
			let adjustment_frequency = T::SerpTesSchedule::get();
			if _now + adjustment_frequency == now {
				// SERP TES (Token Elasticity of Supply).
				// Triggers Serping for all system stablecoins to stabilize stablecoin prices.
				Self::on_serp_tes();
			} else {
				Ok(())
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(T::WeightInfo::auction_serplus())]
		#[transactional]
		pub fn auction_serplus(origin: OriginFor<T>, amount: Balance) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin(origin)?;
			ensure!(
				Self::serplus_pool().saturating_sub(T::SerpAuctionHandler::get_total_serplus_in_auction()) >= amount,
				Error::<T>::SerplusPoolNotEnough,
			);
			T::SerpAuctionHandler::new_serplus_auction(amount)?;
			Ok(().into())
		}

		#[pallet::weight(T::WeightInfo::auction_diamond())]
		#[transactional]
		pub fn auction_diamond(
			origin: OriginFor<T>,
			setter_amount: Balance,
			initial_price: Balance,
		) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin(origin)?;
			T::SerpAuctionHandler::new_diamond_auction(initial_price, setter_amount)?;
			Ok(().into())
		}

		#[pallet::weight(T::WeightInfo::auction_setter())]
		#[transactional]
		pub fn auction_setter(
			origin: OriginFor<T>,
			accepted_currency: CurrencyId,
			currency_amount: Balance,
			initial_price: Balance,
		) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin(origin)?;
			T::SerpAuctionHandler::new_setter_auction(accepted_currency, initial_price, currency_amount)?;
			Ok(().into())
		}

		/// Update parameters related to setter auction under specific
		/// reserve type
		///
		/// The dispatch origin of this call must be `UpdateOrigin`.
		///
		/// - `T::GetSetterCurrencyId::get()`: reserve type
		/// - `serplusbuffer_size`: setter auction maximum size
		#[pallet::weight((T::WeightInfo::set_expected_setter_auction_size(), DispatchClass::Operational))]
		#[transactional]
		pub fn set_expected_setter_auction_size(
			origin: OriginFor<T>,
			currency_id: T::GetSetterCurrencyId::get(),
			size: Balance,
		) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin(origin)?;
			ExpectedSetterAuctionSize::<T>::insert(T::GetSetterCurrencyId::get(), size);
			Self::deposit_event(Event::ExpectedSetterAuctionSizeUpdated(T::GetSetterCurrencyId::get(), size));
			Ok(().into())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Get account of SERP Treasury module.
	pub fn account_id() -> T::AccountId {
		T::ModuleId::get().into_account()
	}

	pub fn adjustment_frequency() -> BlockNumber {
		T::SerpTesSchedule::get()
	}

	/// Get current total serplus of specific currency type in the system.
	pub fn serplus_pool(currency_id: CurrencyId) -> Balance {
		T::Currency::free_balance(currency_id, &Self::account_id())
	}

	/// Get current total serpup of specific currency type in the system.
	pub fn serpup_pool(currency_id: CurrencyId) -> Balance {
		T::Currency::free_balance(currency_id, &Self::account_id())
	}

	/// Get total reserve amount of SERP Treasury module.
	pub fn total_reserve() -> Balance {
		T::Currency::free_balance(T::GetSetterCurrencyId, &Self::account_id())
	}
}

impl<T: Config> SerpTreasury<T::AccountId> for Pallet<T> {
	type Amount = Amount;
	type Balance = Balance;
	type CurrencyId = CurrencyId;
	type BlockNumber = BlockNumber;

	fn get_adjustment_frequency() -> Self::BlockNumber {
		Self::adjustment_frequency()
	}

	/// get surplus amount of serp treasury
	fn get_serplus_pool() -> Self::Balance {
		Self::serplus_pool()
	}

	/// get serpup amount of serp treasury
	fn get_serpup_pool() -> Self::Balance {
		Self::serpup_pool()
	}

	/// get reserve asset amount of serp treasury
	fn get_total_setter() -> Self::Balance {
		Self::total_reserve()
	}

	/// calculate the proportion of specific standard amount for the whole system
	fn get_standard_proportion(amount: Self::Balance, currency_id: Self::CurrencyId) -> Ratio {
		let stable_total_supply = T::Currency::total_issuance(T::GetStableCurrencyId::get());
		Ratio::checked_from_rational(amount, stable_total_supply).unwrap_or_default()
	}

	/// SerpUp ratio for Serplus Auctions / Swaps
	fn get_serplus_serpup(amount: Balance, currency_id: Self::CurrencyId) -> DispatchResult {
		// Serplus SerpUp Pool - 10%
		let serplus_account = &Self::account_id();
		let serplus_ratio = T::SerplusSerpupRatio::get();
		let serplus_propper = amount.checked_mul(&serplus_ratio);
		Self::issue_propper(currency_id, serplus_account, serplus_propper);

		Self::deposit_event(Event::CurrencySerpUpDelivered(amount, currency_id));
		Ok(())
	}

	/// SerpUp ratio for SettPay Cashdrops
	fn get_settpay_serpup(amount: Balance, currency_id: Self::CurrencyId) -> DispatchResult {
		// SettPay SerpUp Pool - 10%
		let settpay_account = T::SettPayTreasuryAcc::get();
		let settpay_ratio = T::SettPaySerpupRatio::get();
		let settpay_propper = amount.checked_mul(&settpay_ratio);
		Self::issue_propper(currency_id, settpay_account, settpay_propper);

		Self::deposit_event(Event::CurrencySerpUpDelivered(amount, currency_id));
		Ok(())
	}

	/// SerpUp ratio for Setheum Treasury
	fn get_treasury_serpup(amount: Balance, currency_id: Self::CurrencyId) -> DispatchResult {
		// Setheum Treasury SerpUp Pool - 10%
		let treasury_account = T::SetheumTreasuryAcc::get();
		let treasury_ratio = T::SetheumTreasurySerpupRatio::get();
		let treasury_propper = amount.checked_mul(&treasury_ratio);
		Self::issue_propper(currency_id, treasury_account, treasury_propper);

		Self::deposit_event(Event::CurrencySerpUpDelivered(amount, currency_id));
		Ok(())
	}

	/// SerpUp ratio for Setheum Investment Fund (SIF) DAO
	fn get_sif_serpup(amount: Balance, currency_id: Self::CurrencyId) -> DispatchResult {
		// SIF SerpUp Pool - 10%
		let sif_account = T::SIFAcc::get();
		let sif_ratio = T::SIFSerpupRatio::get();
		let sif_propper = amount.checked_mul(&sif_ratio);
		Self::issue_propper(currency_id, sif_account, sif_propper);

		Self::deposit_event(Event::CurrencySerpUpDelivered(amount, currency_id));
		Ok(())
	}

	/// SerpUp ratio for Setheum Foundation's Charity Fund
	fn get_charity_fund_serpup(amount: Balance, currency_id: Self::CurrencyId) -> DispatchResult {
		// Charity Fund SerpUp Pool - 10%
		let charity_fund_account = T::CharityFundAcc::get();
		let charity_fund_ratio = T::CharityFundSerpupRatio::get();
		let charity_fund_propper = amount.checked_mul(&charity_fund_ratio);
		Self::issue_propper(currency_id, charity_fund_account, charity_fund_propper);

		Self::deposit_event(Event::CurrencySerpUpDelivered(amount, currency_id));
		Ok(())
	}

	/// issue surplus(stable currencies) for serp treasury
	/// allocates the serp_up and calls on_serpup.
	fn on_system_serpup(currency_id: Self::CurrencyId, amount: Self::Balance) -> DispatchResult {

		let serpup_ratio = Ratio::checked_from_rational(amount, 100); // in percentage (%)
		let total_issuance = T::Currency::total_issuance(currency_id);
		let serpup_balance = total_issuance.checked_mul(&serpup_ratio);
		Self::on_serpup(currency_id, serpup_balance);
	}
	/// issue serpup surplus(stable currencies) to their destinations according to the serpup_ratio.
	fn on_serpup(currency_id: CurrencyId, amount: Amount) -> DispatchResult {
		get_serplus_serpup(amount, currency_id);
		get_settpay_serpup(amount, currency_id);
		get_treasury_serpup(amount, currency_id);
		get_sif_serpup(amount, currency_id);
		get_charity_fund_serpup(amount, currency_id);

		Self::deposit_event(Event::CurrencySerpedUp(amount, currency_id));
		Ok(())
	}

	/// buy back and burn surplus(stable currencies) with auction
	/// allocates the serp_down and calls on_serpdown.
	fn on_system_serpdown(currency_id: Self::CurrencyId, amount: Self::Balance) -> DispatchResult {

		let serpdown_ratio = Ratio::checked_from_rational(amount, 100); // in percentage (%)
		let total_issuance = T::Currency::total_issuance(currency_id);
		let serpdown_balance = total_issuance.checked_mul(&serpdown_ratio);
		Self::on_serpdown(currency_id, serpdown_balance);
	}
	/// buy back and burn surplus(stable currencies) with auction
	/// Create the necessary serp down parameters and starts new auction.
	fn on_serpdown(currency_id: CurrencyId, amount: Amount) -> DispatchResult {
		/// ensure that the currency is a SettCurrency
		ensure!(
			T::StableCurrencyIds::get().contains(&currency_id),
			Error::<T>::InvalidSettCyrrencyType,
		);
		let setter_fixed_price = T::Price::get_setter_fixed_price();

		if currency_id == T::GetSetterCurrencyId::get() {
			let dinar = T::GetNativeCurrencyId::get();
			let dinar_price = T::Price::get_price(&dinar);
			let relative_price = setter_fixed_price.checked_div(&dinar_price);
			let initial_amount = relative_price.checked_mul(&amount);
			/// ensure that the amounts are not zero
			ensure!(
				!initial_amount.is_zero() && !amount.is_zero(),
				Error::<T>::InvalidAmount,
			);
			/// Diamond Auction if it's to serpdown Setter.
			T::SerpAuctionHandler::new_diamond_auction(&initial_amount, &amount)

		} else {
			let settcurrency_fixed_price = T::Price::get_stablecoin_fixed_price(currency_id);
			let relative_price = settcurrency_fixed_price.checked_div(&setter_fixed_price);
			let initial_amount = relative_price.checked_mul(&amount);
			/// ensure that the amounts are not zero
			ensure!(
				!initial_amount.is_zero() && !amount.is_zero(),
				Error::<T>::InvalidAmount,
			);
			/// Setter Auction if it's not to serpdown Setter.
			T::SerpAuctionHandler::new_setter_auction(&initial_amount, &amount, &currency_id)
		}

		Self::deposit_event(Event::CurrencySerpDownTriggered(amount, currency_id));
		Ok(())
	}

	/// Trigger SERP-TES for all stablecoins
	fn on_serp_tes() -> DispatchRsult {
		/// Check all stablecoins stability and serp to stabilise the unstable one(s).
		check_all_stablecoin_stability()
	}

	/// Determines whether to SerpUp or SerpDown based on price swing (+/-)).
	/// positive means "Serp Up", negative means "Serp Down".
	/// Then it calls the necessary option to serp the currency supply (up/down).
	fn serp_tes(currency_id: CurrencyId) -> DispatchResult {

		let differed_amount = T::Prices::get_peg_price_difference(&currency_id);
		let price = T::Prices::get_stablecoin_market_price(&currency_id);

		/// ensure that the differed amount is not zero
		ensure!(
			!differed_amount.is_zero(),
			Self::deposit_event(Event::PriceIsStable(price, currency_id));
		);

		/// if price difference is positive -> SerpUp, else if negative ->SerpDown.
		if differed_amount.is_positive() {
			T::SerpTreasury::on_system_serpup(
				&currency_id, &differed_amount,
				T::Convert::convert((&differed_amount)))?;
		} else if differed_amount.is_negative() {
			T::SerpTreasury::on_system_serpdown(
				&currency_id, &differed_amount,
				T::Convert::convert((&differed_amount)))?;
		}

		Ok(())
	}

	fn check_all_stablecoin_stability() -> DispatchResult {
		/// pegged to US Dollar (USD)
		let peg_one_currency_id: CurrencyId = T::GetSettUSDCurrencyId::get();
		let peg_one_checked_price = Self::get_peg_price_difference(&peg_one_currency_id);
		Self::serp_tes(peg_one_currency_id);

		/// pegged to Pound Sterling (GBP)
		let peg_two_currency_id: CurrencyId = T::GetSettGBPCurrencyId::get();
		let peg_two_checked_price = Self::get_peg_price_difference(&peg_two_currency_id);
		Self::serp_tes(peg_two_currency_id);

		/// pegged to Euro (EUR)
		let peg_three_currency_id: CurrencyId = T::GetSettEURCurrencyId::get();
		let peg_three_checked_price = Self::get_peg_price_difference(&peg_three_currency_id);
		Self::serp_tes(peg_three_currency_id);

		/// pegged to Kuwaiti Dinar (KWD)
		let peg_four_currency_id: CurrencyId = T::GetSettKWDCurrencyId::get();
		let peg_four_checked_price = Self::get_peg_price_difference(&peg_four_currency_id);
		Self::serp_tes(peg_four_currency_id);

		/// pegged to Jordanian Dinar (JOD)
		let peg_five_currency_id: CurrencyId = T::GetSettJODCurrencyId::get();
		let peg_five_checked_price = Self::get_peg_price_difference(&peg_five_currency_id);
		Self::serp_tes(peg_five_currency_id);

		/// pegged to Bahraini Dirham (BHD)
		let peg_six_currency_id: CurrencyId = T::GetSettBHDCurrencyId::get();
		let peg_six_checked_price = Self::get_peg_price_difference(&peg_six_currency_id);
		Self::serp_tes(peg_six_currency_id);

		/// pegged to Cayman Islands Dollar (KYD)
		let peg_seven_currency_id: CurrencyId = T::GetSettKYDCurrencyId::get();
		let peg_seven_checked_price = Self::get_peg_price_difference(&peg_seven_currency_id);
		Self::serp_tes(peg_seven_currency_id);

		/// pegged to Omani Riyal (OMR)
		let peg_eight_currency_id: CurrencyId = T::GetSettOMRCurrencyId::get();
		let peg_eight_checked_price = Self::get_peg_price_difference(&peg_eight_currency_id);
		Self::serp_tes(peg_eight_currency_id);

		/// pegged to Swiss Franc (CHF)
		let peg_nine_currency_id: CurrencyId = T::GetSettCHFCurrencyId::get();
		let peg_nine_checked_price = Self::get_peg_price_difference(&peg_nine_currency_id);
		Self::serp_tes(peg_nine_currency_id);

		/// pegged to Gibraltar Pound (GIP)
		let peg_ten_currency_id: CurrencyId = T::GetSettGIPCurrencyId::get();
		let peg_ten_checked_price = Self::get_peg_price_difference(&peg_ten_currency_id);
		Self::serp_tes(peg_ten_currency_id);

		Ok(())
	}
	/// TODO: update to `currency_id` which is any `SettCurrency`.
	fn issue_standard(currency_id: CurrencyId, who: &T::AccountId, standard: Self::Balance) -> DispatchResult {
		T::Currency::deposit(currency_id, who, standard)?;
		Ok(())
	}

	/// TODO: update to `currency_id` which is any `SettCurrency`.
	fn burn_standard(currency_id: CurrencyId, who: &T::AccountId, standard: Self::Balance) -> DispatchResult {
		T::Currency::withdraw(currency_id, who, standard)
	}

	fn issue_propper(currency_id: CurrencyId, who: &T::AccountId, propper: Self::Balance) -> DispatchResult {
		T::Currency::deposit(currency_id, who, propper)?;
		Ok(())
	}

	fn burn_propper(currency_id: CurrencyId, who: &T::AccountId, propper: Self::Balance) -> DispatchResult {
		T::Currency::withdraw(currency_id, who, propper)
	}
	fn issue_setter(who: &T::AccountId, setter: Self::Balance) -> DispatchResult {
		T::Currency::deposit(T::GetSetterCurrencyId::get(), who, setter)?;
		Ok(())
	}

	fn burn_setter(who: &T::AccountId, setter: Self::Balance) -> DispatchResult {
		T::Currency::withdraw(T::GetSetterCurrencyId::get(), who, setter)
	}

	/// Issue Dexer (`SDEX` in Setheum or `HALAL` in Neom). `dexer` here just referring to the DEX token balance.
	fn issue_dexer(who: &T::AccountId, dexer: Self::Balance) -> DispatchResult {
		T::Currency::deposit(T::GetDexerCurrencyId::get(), who, dexer)?;
		Ok(())
	}

	/// Burn Dexer (`SDEX` in Setheum or `HALAL` in Neom). `dexer` here just referring to the DEX token balance.
	fn burn_dexer(who: &T::AccountId, dexer: Self::Balance) -> DispatchResult {
		T::Currency::withdraw(T::GetDexerCurrencyId::get(), who, dexer)
	}

	fn deposit_serplus(currency_id: CurrencyId, from: &T::AccountId, serplus: Self::Balance) -> DispatchResult {
		T::Currency::transfer(currency_id, from, &Self::account_id(), serplus)
	}

	fn deposit_reserve(from: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		T::Currency::transfer(T::GetSetterCurrencyId::get(), from, &Self::account_id(), amount)
	}

	fn burn_reserve(to: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		T::Currency::transfer(T::GetSetterCurrencyId::get(), &Self::account_id(), to, amount)
	}

	fn withdraw_reserve(to: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		T::Currency::transfer(T::GetSetterCurrencyId::get(), &Self::account_id(), to, amount)
	}
}

impl<T: Config> SerpTreasuryExtended<T::AccountId> for Pallet<T> {
	/// Swap exact amount of setter in auction to settcurrency,
	/// return actual target settcurrency amount
	fn swap_exact_setter_in_auction_to_settcurrency(
		currency_id: T::GetSetterCurrencyId::get(),
		supply_amount: Balance,
		min_target_amount: Balance,
		price_impact_limit: Option<Ratio>,
	) -> sp_std::result::Result<Balance, DispatchError> {
		let settcurrency_currency_id: CurrencyId;
		ensure!(
			T::SerpAuctionHandler::get_total_setter_in_auction() >= supply_amount,
			Error::<T>::SetterNotEnough,
		);
		ensure!(
			T::StableCurrencyIds::get().contains(settcurrency_currency_id),
			Error::<T>::InvalidSettCyrrencyType,
		);

		T::DEX::swap_with_exact_supply(
			&Self::account_id(),
			&[T::GetSetterCurrencyId::get(), settcurrency_currency_id],
			supply_amount,
			min_target_amount,
			price_impact_limit,
		)
	}
}

#[cfg(feature = "std")]
impl GenesisConfig {
	/// Direct implementation of `GenesisBuild::build_storage`.
	///
	/// Kept in order not to break dependency.
	pub fn build_storage<T: Config>(&self) -> Result<sp_runtime::Storage, String> {
		<Self as GenesisBuild<T>>::build_storage(self)
	}

	/// Direct implementation of `GenesisBuild::assimilate_storage`.
	///
	/// Kept in order not to break dependency.
	pub fn assimilate_storage<T: Config>(&self, storage: &mut sp_runtime::Storage) -> Result<(), String> {
		<Self as GenesisBuild<T>>::assimilate_storage(self, storage)
	}
}
