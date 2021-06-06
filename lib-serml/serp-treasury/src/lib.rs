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
use primitives::{Balance, CurrencyId};
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
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {}

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
	type Balance = Balance;
	type CurrencyId = CurrencyId;

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
	fn get_standard_proportion(amount: Self::Balance) -> Ratio {
		let stable_total_supply = T::Currency::total_issuance(T::GetStableCurrencyId::get());
		Ratio::checked_from_rational(amount, stable_total_supply).unwrap_or_default()
	}

	/// SerpUp ratio for Serplus Auctions / Swaps
	fn get_serplus_serpup(amount: Balance) -> DispatchResult {
		// Serplus SerpUp Pool - 10%
		let serplus_account = &Self::account_id();
		let serplus_ratio = T::SerplusSerpupRatio::get();
		let serplus_propper = amount.checked_mul(&serplus_ratio);
		Self::issue_propper(currency_id, serplus_account, serplus_propper);
		Ok(())
	}

	/// SerpUp ratio for SettPay Cashdrops
	fn get_settpay_serpup(amount: Balance) -> DispatchResult {
		// SettPay SerpUp Pool - 10%
		let settpay_account = T::SettPayTreasuryAcc::get();
		let settpay_ratio = T::SettPaySerpupRatio::get();
		let settpay_propper = amount.checked_mul(&settpay_ratio);
		Self::issue_propper(currency_id, settpay_account, settpay_propper);
		Ok(())
	}

	/// SerpUp ratio for Setheum Treasury
	fn get_treasury_serpup(amount: Balance) -> DispatchResult {
		// Setheum Treasury SerpUp Pool - 10%
		let treasury_account = T::SetheumTreasuryAcc::get();
		let treasury_ratio = T::SetheumTreasurySerpupRatio::get();
		let treasury_propper = amount.checked_mul(&treasury_ratio);
		Self::issue_propper(currency_id, treasury_account, treasury_propper);
		Ok(())
	}

	/// SerpUp ratio for Setheum Investment Fund (SIF) DAO
	fn get_sif_serpup(amount: Balance) -> DispatchResult {
		// SIF SerpUp Pool - 10%
		let sif_account = T::SIFAcc::get();
		let sif_ratio = T::SIFSerpupRatio::get();
		let sif_propper = amount.checked_mul(&sif_ratio);
		Self::issue_propper(currency_id, sif_account, sif_propper);
		Ok(())
	}

	/// SerpUp ratio for Setheum Foundation's Charity Fund
	fn get_charity_fund_serpup(amount: Balance) -> DispatchResult {
		// Charity Fund SerpUp Pool - 10%
		let charity_fund_account = T::CharityFundAcc::get();
		let charity_fund_ratio = T::CharityFundSerpupRatio::get();
		let charity_fund_propper = amount.checked_mul(&charity_fund_ratio);
		Self::issue_propper(currency_id, charity_fund_account, charity_fund_propper);
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
	/// issue surplus(stable currencies) for serp treasury
	/// calls for what to do before serpup then calls on_system_serpup.
	fn on_serpup(currency_id: CurrencyId, amount: Amount) -> DispatchResult {
		get_serplus_serpup(amount);
		get_settpay_serpup(amount);
		get_treasury_serpup(amount);
		get_sif_serpup(amount);
		get_charity_fund_serpup(amount);
		Ok(())
	}

	/// buy back and burn surplus(stable currencies) with auction / dex-swap
	/// allocates the serp_down and calls auction depending on the currency_id.
	fn on_system_serpdown(currency_id: Self::CurrencyId, amount: Self::Balance) -> DispatchResult {
		let origin: OriginFor<T>;
		let amount: Balance;
		let target: Balance;
		let splited: bool;
		Self::auction_setter(origin, amount, target, true)
	}

	/// issue surplus(stable currencies) for serp treasury
	/// allocates the serp_up and calls on_serpup.
	fn on_system_serpdown(currency_id: Self::CurrencyId, amount: Self::Balance) -> DispatchResult {

		let serpdown_ratio = Ratio::checked_from_rational(amount, 100); // in percentage (%)
		let total_issuance = T::Currency::total_issuance(currency_id);
		let serpdown_balance = total_issuance.checked_mul(&serpdown_ratio);
		Self::on_serpdown(currency_id, serpdown_balance);
	}
	/// issue surplus(stable currencies) for serp treasury
	/// calls for what to do before serpup then calls on_system_serpup.
	fn on_serpdown(currency_id: CurrencyId, amount: Amount) -> DispatchResult {
		ensure!(
			T::SettCurrencyIds::get().contains(currency_id),
			Error::<T>::InvalidSettCyrrencyType,
		);
		if currency_id == T::GetSetterCurrencyId::get() {

		} else {

		}

		Ok(())
	}


		///
		/// TODO: SerpTreasury should specify that:-
		///
		/// fn serp_tes(currency_id: CurrencyId, peg_price_difference_amount: Amount) -> DispatchResult {
		/// 	if peg_price_difference_amount.is_positive() {
		/// 		T::SerpTreasury::on_serpup(currency_id, peg_price_difference_amount, T::Convert::convert((peg_price_difference_amount)))?;
		/// 	} else if peg_price_difference_amount.is_negative() {
		/// 		T::SerpTreasury::on_serpdown(currency_id, peg_price_difference_amount, T::Convert::convert((peg_price_difference_amount)))?;
		/// 	}
		/// }
		///

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
			T::SettCurrencyIds::get().contains(settcurrency_currency_id),
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
