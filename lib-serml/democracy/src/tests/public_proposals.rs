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

//! The tests for the public proposal queue.

use super::*;

#[test]
fn backing_for_proposal_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(propose_set_balance_and_note(1, 2, 2));
		assert_ok!(propose_set_balance_and_note(1, 4, 4));
		assert_ok!(propose_set_balance_and_note(1, 3, 3));
		assert_eq!(Democracy::backing_for(0), Some(DNAR, 2));
		assert_eq!(Democracy::backing_for(1), Some(DNAR, 4));
		assert_eq!(Democracy::backing_for(2), Some(DNAR, 3));
	});
}

#[test]
fn deposit_for_proposals_should_be_taken() {
	new_test_ext().execute_with(|| {
		assert_ok!(propose_set_balance_and_note(1, 2, 5));
		assert_ok!(Democracy::second(Origin::signed(2), 0, u32::MAX));
		assert_ok!(Democracy::second(Origin::signed(5), 0, u32::MAX));
		assert_ok!(Democracy::second(Origin::signed(5), 0, u32::MAX));
		assert_ok!(Democracy::second(Origin::signed(5), 0, u32::MAX));
		assert_eq!(Tokens::free_balance(DNAR, 1), 5);
		assert_eq!(Tokens::free_balance(DNAR, 2), 15);
		assert_eq!(Tokens::free_balance(DNAR, 5), 35);
	});
}

#[test]
fn deposit_for_proposals_should_be_returned() {
	new_test_ext().execute_with(|| {
		assert_ok!(propose_set_balance_and_note(1, 2, 5));
		assert_ok!(Democracy::second(Origin::signed(2), 0, u32::MAX));
		assert_ok!(Democracy::second(Origin::signed(5), 0, u32::MAX));
		assert_ok!(Democracy::second(Origin::signed(5), 0, u32::MAX));
		assert_ok!(Democracy::second(Origin::signed(5), 0, u32::MAX));
		fast_forward_to(3);
		assert_eq!(Tokens::free_balance(DNAR, 1), 10);
		assert_eq!(Tokens::free_balance(DNAR, 2), 20);
		assert_eq!(Tokens::free_balance(DNAR, 5), 50);
	});
}

#[test]
fn proposal_with_deposit_below_minimum_should_not_work() {
	new_test_ext().execute_with(|| {
		assert_noop!(propose_set_balance(1, 2, 0), Error::<Test>::ValueLow);
	});
}

#[test]
fn poor_proposer_should_not_work() {
	new_test_ext().execute_with(|| {
		assert_noop!(propose_set_balance(1, 2, 11), BalancesError::<Test, _>::InsufficientBalance);
	});
}

#[test]
fn poor_seconder_should_not_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(propose_set_balance_and_note(2, 2, 11));
		assert_noop!(
			Democracy::second(Origin::signed(1), 0, u32::MAX),
			BalancesError::<Test, _>::InsufficientBalance
		);
	});
}

#[test]
fn invalid_seconds_upper_bound_should_not_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(propose_set_balance_and_note(1, 2, 5));
		assert_noop!(
			Democracy::second(Origin::signed(2), 0, 0),
			Error::<Test>::WrongUpperBound
		);
	});
}

#[test]
fn cancel_proposal_should_work() {
	new_test_ext().execute_with(|| {
		System::set_block_number(0);
		assert_ok!(propose_set_balance_and_note(1, 2, 2));
		assert_ok!(propose_set_balance_and_note(1, 4, 4));
		assert_noop!(Democracy::cancel_proposal(Origin::signed(1), 0), BadOrigin);
		assert_ok!(Democracy::cancel_proposal(Origin::root(), 0));
		assert_eq!(Democracy::backing_for(0), None);
		assert_eq!(Democracy::backing_for(1), Some(DNAR, 4));
	});
}

#[test]
fn blacklisting_should_work() {
	new_test_ext().execute_with(|| {
		System::set_block_number(0);
		let hash = set_balance_proposal_hash(2);

		assert_ok!(propose_set_balance_and_note(1, 2, 2));
		assert_ok!(propose_set_balance_and_note(1, 4, 4));

		assert_noop!(Democracy::blacklist(Origin::signed(1), hash.clone(), None), BadOrigin);
		assert_ok!(Democracy::blacklist(Origin::root(), hash, None));

		assert_eq!(Democracy::backing_for(0), None);
		assert_eq!(Democracy::backing_for(1), Some(DNAR, 4));

		assert_noop!(propose_set_balance_and_note(1, 2, 2), Error::<Test>::ProposalBlacklisted);

		fast_forward_to(2);

		let hash = set_balance_proposal_hash(4);
		assert_ok!(Democracy::referendum_status(0));
		assert_ok!(Democracy::blacklist(Origin::root(), hash, Some(0)));
		assert_noop!(Democracy::referendum_status(0), Error::<Test>::ReferendumInvalid);
	});
}

#[test]
fn runners_up_should_come_after() {
	new_test_ext().execute_with(|| {
		System::set_block_number(0);
		assert_ok!(propose_set_balance_and_note(1, 2, 2));
		assert_ok!(propose_set_balance_and_note(1, 4, 4));
		assert_ok!(propose_set_balance_and_note(1, 3, 3));
		fast_forward_to(2);
		assert_ok!(Democracy::vote(Origin::signed(1), 0, aye(1)));
		fast_forward_to(4);
		assert_ok!(Democracy::vote(Origin::signed(1), 1, aye(1)));
		fast_forward_to(6);
		assert_ok!(Democracy::vote(Origin::signed(1), 2, aye(1)));
	});
}
