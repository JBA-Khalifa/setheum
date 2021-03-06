// This file is part of Setheum.

// Copyright (C) 2019-2021 Setheum Labs.
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

//! Unit tests for the serp auction module.

#![cfg(test)]

use super::*;
use frame_support::{assert_noop, assert_ok};
use mock::{Event, *};

#[test]
fn get_auction_time_to_close_works() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(SerpAuctionManagerModule::get_auction_time_to_close(2000, 1), 100);
		assert_eq!(SerpAuctionManagerModule::get_auction_time_to_close(2001, 1), 50);
	});
}

#[test]
fn setter_auction_methods() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(SerpAuctionManagerModule::new_setter_auction(200, 100, USDJ));
		let setter_auction = SerpAuctionManagerModule::setter_auctions(0).unwrap();
		assert_eq!(setter_auction.amount_for_sale(0, 100), 200);
		assert_eq!(setter_auction.amount_for_sale(100, 200), 100);
		assert_eq!(setter_auction.amount_for_sale(200, 1000), 40);
	});
}

#[test]
fn diamond_auction_methods() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(SerpAuctionManagerModule::new_diamond_auction(200, 100));
		let diamond_auction = SerpAuctionManagerModule::diamond_auctions(0).unwrap();
		assert_eq!(diamond_auction.amount_for_sale(0, 100), 200);
		assert_eq!(diamond_auction.amount_for_sale(100, 200), 100);
		assert_eq!(diamond_auction.amount_for_sale(200, 1000), 40);
	});
}

#[test]
fn new_setter_auction_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_noop!(
			SerpAuctionManagerModule::new_setter_auction(0, 100, USDJ),
			Error::<Runtime>::InvalidAmount,
		);
		assert_noop!(
			SerpAuctionManagerModule::new_setter_auction(200, 0, USDJ),
			Error::<Runtime>::InvalidAmount,
		);

		assert_ok!(SerpAuctionManagerModule::new_setter_auction(200, 100, USDJ));
		System::assert_last_event(Event::serp_auction(crate::Event::NewSetterAuction(0, 200, 100)));

		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 100);
		assert_eq!(AuctionModule::auctions_index(), 1);

		assert_noop!(
			SerpAuctionManagerModule::new_setter_auction(200, Balance::max_value(), USDJ),
			Error::<Runtime>::InvalidAmount,
		);
	});
}

#[test]
fn new_diamond_auction_work() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_noop!(
			SerpAuctionManagerModule::new_diamond_auction(0, 100),
			Error::<Runtime>::InvalidAmount,
		);
		assert_noop!(
			SerpAuctionManagerModule::new_diamond_auction(200, 0),
			Error::<Runtime>::InvalidAmount,
		);

		assert_ok!(SerpAuctionManagerModule::new_diamond_auction(200, 100));
		System::assert_last_event(Event::serp_auction(crate::Event::NewDiamondAuction(0, 200, 100)));

		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 100);
		assert_eq!(AuctionModule::auctions_index(), 1);

		assert_noop!(
			SerpAuctionManagerModule::new_diamond_auction(200, Balance::max_value()),
			Error::<Runtime>::InvalidAmount,
		);
	});
}

#[test]
fn new_serplus_auction_work() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_noop!(
			SerpAuctionManagerModule::new_serplus_auction(0, USDJ),
			Error::<Runtime>::InvalidAmount,
		);

		assert_ok!(SerpAuctionManagerModule::new_serplus_auction(100, USDJ));
		System::assert_last_event(Event::serp_auction(crate::Event::NewSerplusAuction(0, 100, USDJ)));

		assert_eq!(SerpAuctionManagerModule::total_diamond_in_auction(), 100);
		assert_eq!(AuctionModule::auctions_index(), 1);

		assert_noop!(
			SerpAuctionManagerModule::new_serplus_auction(Balance::max_value(), USDJ),
			Error::<Runtime>::InvalidAmount,
		);
	});
}

#[test]
fn setter_auction_bid_handler_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			SerpAuctionManagerModule::setter_auction_bid_handler(1, 0, USDJ, (BOB, 99), None),
			Error::<Runtime>::AuctionNonExistent,
		);

		assert_ok!(SerpAuctionManagerModule::new_setter_auction(200, 100, USDJ));
		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 100);
		assert_eq!(SerpAuctionManagerModule::setter_auctions(0).unwrap().amount, 200);
		assert_eq!(SerpTreasuryModule::serplus_pool(), 0);
		assert_eq!(Tokens::free_balance(SETT, &BOB), 1000);

		let bob_ref_count_0 = System::consumers(&BOB);

		assert_noop!(
			SerpAuctionManagerModule::setter_auction_bid_handler(1, 0, USDJ, (BOB, 99), None),
			Error::<Runtime>::InvalidBidPrice,
		);
		assert_eq!(
			SerpAuctionManagerModule::setter_auction_bid_handler(1, 0, USDJ, (BOB, 100), None).is_ok(),
			true
		);
		assert_eq!(SerpAuctionManagerModule::setter_auctions(0).unwrap().amount, 200);
		assert_eq!(SerpTreasuryModule::serplus_pool(), 100);
		assert_eq!(Tokens::free_balance(SETT, &BOB), 900);

		let bob_ref_count_1 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_1, bob_ref_count_0 + 1);
		let carol_ref_count_0 = System::consumers(&CAROL);

		assert_eq!(
			SerpAuctionManagerModule::setter_auction_bid_handler(2, 0, USDJ, (CAROL, 200), Some((BOB, 100))).is_ok(),
			true
		);
		assert_eq!(SerpAuctionManagerModule::setter_auctions(0).unwrap().amount, 100);
		assert_eq!(SerpTreasuryModule::serplus_pool(), 100);
		assert_eq!(Tokens::free_balance(SETT, &BOB), 1000);
		assert_eq!(Tokens::free_balance(SETT, &CAROL), 900);
		let bob_ref_count_2 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_2, bob_ref_count_1 - 1);
		let carol_ref_count_1 = System::consumers(&CAROL);
		assert_eq!(carol_ref_count_1, carol_ref_count_0 + 1);
	});
}

#[test]
fn diamond_auction_bid_handler_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			SerpAuctionManagerModule::diamond_auction_bid_handler(1, 0, (BOB, 99), None),
			Error::<Runtime>::AuctionNonExistent,
		);

		assert_ok!(SerpAuctionManagerModule::new_diamond_auction(200, 100));
		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 100);
		assert_eq!(SerpAuctionManagerModule::diamond_auctions(0).unwrap().amount, 200);
		assert_eq!(SerpTreasuryModule::serplus_pool(), 0);
		assert_eq!(Tokens::free_balance(DNAR, &BOB), 1000);

		let bob_ref_count_0 = System::consumers(&BOB);

		assert_noop!(
			SerpAuctionManagerModule::diamond_auction_bid_handler(1, 0, (BOB, 99), None),
			Error::<Runtime>::InvalidBidPrice,
		);
		assert_eq!(
			SerpAuctionManagerModule::diamond_auction_bid_handler(1, 0, (BOB, 100), None).is_ok(),
			true
		);
		assert_eq!(SerpAuctionManagerModule::diamond_auctions(0).unwrap().amount, 200);
		assert_eq!(SerpTreasuryModule::serplus_pool(), 100);
		assert_eq!(Tokens::free_balance(DNAR, &BOB), 900);

		let bob_ref_count_1 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_1, bob_ref_count_0 + 1);
		let carol_ref_count_0 = System::consumers(&CAROL);

		assert_eq!(
			SerpAuctionManagerModule::diamond_auction_bid_handler(2, 0, (CAROL, 200), Some((BOB, 100))).is_ok(),
			true
		);
		assert_eq!(SerpAuctionManagerModule::diamond_auctions(0).unwrap().amount, 100);
		assert_eq!(SerpTreasuryModule::serplus_pool(), 100);
		assert_eq!(Tokens::free_balance(DNAR, &BOB), 1000);
		assert_eq!(Tokens::free_balance(DNAR, &CAROL), 900);
		let bob_ref_count_2 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_2, bob_ref_count_1 - 1);
		let carol_ref_count_1 = System::consumers(&CAROL);
		assert_eq!(carol_ref_count_1, carol_ref_count_0 + 1);
	});
}

#[test]
fn serplus_auction_bid_handler_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			SerpAuctionManagerModule::serplus_auction_bid_handler(1, 0, (BOB, 99), None),
			Error::<Runtime>::AuctionNonExistent,
		);

		assert_ok!(SerpAuctionManagerModule::new_serplus_auction(100, USDJ));
		assert_eq!(Tokens::free_balance(USDJ, &BOB), 1000);

		let bob_ref_count_0 = System::consumers(&BOB);

		assert_eq!(
			SerpAuctionManagerModule::serplus_auction_bid_handler(1, 0, (BOB, 50), None).is_ok(),
			true
		);
		assert_eq!(Tokens::free_balance(USDJ, &BOB), 950);
		assert_eq!(Tokens::free_balance(USDJ, &CAROL), 1000);

		let bob_ref_count_1 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_1, bob_ref_count_0 + 1);
		let carol_ref_count_0 = System::consumers(&CAROL);

		assert_noop!(
			SerpAuctionManagerModule::serplus_auction_bid_handler(2, 0, (CAROL, 51), Some((BOB, 50))),
			Error::<Runtime>::InvalidBidPrice,
		);
		assert_eq!(
			SerpAuctionManagerModule::serplus_auction_bid_handler(2, 0, (CAROL, 55), Some((BOB, 50))).is_ok(),
			true
		);
		assert_eq!(Tokens::free_balance(USDJ, &BOB), 1000);
		assert_eq!(Tokens::free_balance(USDJ, &CAROL), 945);
		let bob_ref_count_2 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_2, bob_ref_count_1 - 1);
		let carol_ref_count_1 = System::consumers(&CAROL);
		assert_eq!(carol_ref_count_1, carol_ref_count_0 + 1);
	});
}

#[test]
fn bid_when_soft_cap_for_setter_auction_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(SerpAuctionManagerModule::new_setter_auction(200, 100, USDJ));
		assert_eq!(
			SerpAuctionManagerModule::on_new_bid(1, 0, (BOB, 100), None).auction_end_change,
			Change::NewValue(Some(101))
		);
		assert_eq!(
			SerpAuctionManagerModule::on_new_bid(2001, 0, (CAROL, 105), Some((BOB, 100))).accept_bid,
			false
		);
		assert_eq!(
			SerpAuctionManagerModule::on_new_bid(2001, 0, (CAROL, 110), Some((BOB, 100))).auction_end_change,
			Change::NewValue(Some(2051))
		);
	});
}

#[test]
fn bid_when_soft_cap_for_diamond_auction_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(SerpAuctionManagerModule::new_diamond_auction(200, 100));
		assert_eq!(
			SerpAuctionManagerModule::on_new_bid(1, 0, (BOB, 100), None).auction_end_change,
			Change::NewValue(Some(101))
		);
		assert_eq!(
			SerpAuctionManagerModule::on_new_bid(2001, 0, (CAROL, 105), Some((BOB, 100))).accept_bid,
			false
		);
		assert_eq!(
			SerpAuctionManagerModule::on_new_bid(2001, 0, (CAROL, 110), Some((BOB, 100))).auction_end_change,
			Change::NewValue(Some(2051))
		);
	});
}

#[test]
fn bid_when_soft_cap_for_serplus_auction_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(SerpAuctionManagerModule::new_serplus_auction(100, USDJ));
		assert_eq!(
			SerpAuctionManagerModule::on_new_bid(1, 0, (BOB, 100), None).auction_end_change,
			Change::NewValue(Some(101))
		);
		assert_eq!(
			SerpAuctionManagerModule::on_new_bid(2001, 0, (CAROL, 105), Some((BOB, 100))).accept_bid,
			false
		);
		assert_eq!(
			SerpAuctionManagerModule::on_new_bid(2001, 0, (CAROL, 110), Some((BOB, 100))).auction_end_change,
			Change::NewValue(Some(2051))
		);
	});
}

#[test]
fn setter_auction_end_handler_without_bid() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(SerpAuctionManagerModule::new_setter_auction(300, 100, USDJ));
		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 100);

		assert_eq!(SerpAuctionManagerModule::setter_auctions(0).is_some(), true);
		SerpAuctionManagerModule::on_auction_ended(0, None);
		System::assert_last_event(Event::serp_auction(crate::Event::CancelAuction(0)));

		assert_eq!(SerpAuctionManagerModule::setter_auctions(0), None);
		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 0);
	});
}

#[test]
fn setter_auction_end_handler_with_bid() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(SerpAuctionManagerModule::new_setter_auction(300, 100, USDJ));
		assert_eq!(
			SerpAuctionManagerModule::setter_auction_bid_handler(1, 0, USDJ, (BOB, 100), None).is_ok(),
			true
		);
		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 100);
		assert_eq!(Tokens::free_balance(USDJ, &BOB), 900);
		assert_eq!(Tokens::free_balance(SETT, &BOB), 1000);

		let bob_ref_count_0 = System::consumers(&BOB);

		assert_eq!(SerpAuctionManagerModule::setter_auctions(0).is_some(), true);
		SerpAuctionManagerModule::on_auction_ended(0, Some((BOB, 100)));
		System::assert_last_event(Event::serp_auction(crate::Event::SetterAuctionDealt(0, 300, BOB, 100)));

		assert_eq!(Tokens::free_balance(SETT, &BOB), 1300);
		assert_eq!(Tokens::total_issuance(SETT), 3300);
		assert_eq!(SerpAuctionManagerModule::setter_auctions(0), None);
		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 0);

		let bob_ref_count_1 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_1, bob_ref_count_0 - 1);
	});
}

#[test]
fn diamond_auction_end_handler_without_bid() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(SerpAuctionManagerModule::new_diamond_auction(300, 100));
		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 100);

		assert_eq!(SerpAuctionManagerModule::diamond_auctions(0).is_some(), true);
		SerpAuctionManagerModule::on_auction_ended(0, None);
		System::assert_last_event(Event::serp_auction(crate::Event::CancelAuction(0)));

		assert_eq!(SerpAuctionManagerModule::diamond_auctions(0), None);
		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 0);
	});
}

#[test]
fn diamond_auction_end_handler_with_bid() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(SerpAuctionManagerModule::new_diamond_auction(300, 100));
		assert_eq!(
			SerpAuctionManagerModule::diamond_auction_bid_handler(1, 0, (BOB, 100), None).is_ok(),
			true
		);
		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 100);
		assert_eq!(Tokens::free_balance(USDJ, &BOB), 900);
		assert_eq!(Tokens::free_balance(DNAR, &BOB), 1000);

		let bob_ref_count_0 = System::consumers(&BOB);

		assert_eq!(SerpAuctionManagerModule::diamond_auctions(0).is_some(), true);
		SerpAuctionManagerModule::on_auction_ended(0, Some((BOB, 100)));
		System::assert_last_event(Event::serp_auction(crate::Event::DiamondAuctionDealt(0, 300, BOB, 100)));

		assert_eq!(Tokens::free_balance(DNAR, &BOB), 1300);
		assert_eq!(Tokens::total_issuance(DNAR), 3300);
		assert_eq!(SerpAuctionManagerModule::diamond_auctions(0), None);
		assert_eq!(SerpAuctionManagerModule::total_standard_in_auction(), 0);

		let bob_ref_count_1 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_1, bob_ref_count_0 - 1);
	});
}

#[test]
fn serplus_auction_end_handler_without_bid() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(SerpAuctionManagerModule::new_serplus_auction(100, USDJ));
		assert_eq!(SerpAuctionManagerModule::total_diamond_in_auction(), 100);

		assert_eq!(SerpAuctionManagerModule::serplus_auctions(0).is_some(), true);
		SerpAuctionManagerModule::on_auction_ended(0, None);
		System::assert_last_event(Event::serp_auction(crate::Event::CancelAuction(0)));

		assert_eq!(SerpAuctionManagerModule::serplus_auctions(0), None);
		assert_eq!(SerpAuctionManagerModule::total_diamond_in_auction(), 0);
	});
}

#[test]
fn serplus_auction_end_handler_with_bid() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(SerpTreasuryModule::on_system_serpup(100));
		assert_ok!(SerpAuctionManagerModule::new_serplus_auction(100, USDJ));
		assert_eq!(
			SerpAuctionManagerModule::serplus_auction_bid_handler(1, 0, (BOB, 500), None).is_ok(),
			true
		);
		assert_eq!(SerpAuctionManagerModule::total_diamond_in_auction(), 100);
		assert_eq!(Tokens::free_balance(USDJ, &BOB), 1000);
		assert_eq!(Tokens::free_balance(DNAR, &BOB), 500);
		assert_eq!(Tokens::total_issuance(DNAR), 2500);

		let bob_ref_count_0 = System::consumers(&BOB);

		assert_eq!(SerpAuctionManagerModule::serplus_auctions(0).is_some(), true);
		SerpAuctionManagerModule::on_auction_ended(0, Some((BOB, 500)));
		System::assert_last_event(Event::serp_auction(crate::Event::SerplusAuctionDealt(0, 100, BOB, 500)));

		assert_eq!(SerpAuctionManagerModule::serplus_auctions(0), None);
		assert_eq!(SerpAuctionManagerModule::total_diamond_in_auction(), 0);
		assert_eq!(Tokens::free_balance(USDJ, &BOB), 1100);
		assert_eq!(Tokens::total_issuance(DNAR), 2500);

		let bob_ref_count_1 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_1, bob_ref_count_0 - 1);
	});
}

#[test]
fn swap_bidders_works() {
	ExtBuilder::default().build().execute_with(|| {
		let alice_ref_count_0 = System::consumers(&ALICE);
		let bob_ref_count_0 = System::consumers(&BOB);

		SerpAuctionManagerModule::swap_bidders(&BOB, None);

		let bob_ref_count_1 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_1, bob_ref_count_0 + 1);

		SerpAuctionManagerModule::swap_bidders(&ALICE, Some(&BOB));

		let alice_ref_count_1 = System::consumers(&ALICE);
		assert_eq!(alice_ref_count_1, alice_ref_count_0 + 1);
		let bob_ref_count_2 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_2, bob_ref_count_1 - 1);

		SerpAuctionManagerModule::swap_bidders(&BOB, Some(&ALICE));

		let alice_ref_count_2 = System::consumers(&ALICE);
		assert_eq!(alice_ref_count_2, alice_ref_count_1 - 1);
		let bob_ref_count_3 = System::consumers(&BOB);
		assert_eq!(bob_ref_count_3, bob_ref_count_2 + 1);
	});
}
