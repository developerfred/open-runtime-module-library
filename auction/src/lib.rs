#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{decl_error, decl_event, decl_module, decl_storage, ensure, IterableStorageDoubleMap, Parameter};
use frame_system::{self as system, ensure_signed};
use orml_traits::{Auction, AuctionHandler, AuctionInfo};
use sp_runtime::{
	traits::{AtLeast32Bit, MaybeSerializeDeserialize, Member, One, Zero},
	DispatchResult,
};

mod mock;
mod tests;

pub trait Trait: frame_system::Trait {
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
	type Balance: Parameter + Member + AtLeast32Bit + Default + Copy + MaybeSerializeDeserialize;
	type AuctionId: Parameter + Member + AtLeast32Bit + Default + Copy + MaybeSerializeDeserialize;
	type Handler: AuctionHandler<Self::AccountId, Self::Balance, Self::BlockNumber, Self::AuctionId>;
}

decl_event!(
	pub enum Event<T> where
		<T as frame_system::Trait>::AccountId,
		<T as Trait>::Balance,
		<T as Trait>::AuctionId,
	{
		Bid(AuctionId, AccountId, Balance),
	}
);

decl_storage! {
	trait Store for Module<T: Trait> as Auction {
		pub Auctions get(fn auctions): map hasher(twox_64_concat) T::AuctionId => Option<AuctionInfo<T::AccountId, T::Balance, T::BlockNumber>>;
		pub AuctionsIndex get(fn auctions_index): T::AuctionId;
		pub AuctionEndTime get(fn auction_end_time): double_map hasher(twox_64_concat) T::BlockNumber, hasher(twox_64_concat) T::AuctionId => Option<bool>;
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		type Error = Error<T>;

		fn deposit_event() = default;

		#[weight = frame_support::weights::SimpleDispatchInfo::default()]
		pub fn bid(origin, id: T::AuctionId, #[compact] value: T::Balance) {
			let from = ensure_signed(origin)?;

			let mut auction = <Auctions<T>>::get(id).ok_or(Error::<T>::AuctionNotExist)?;

			let block_number = <frame_system::Module<T>>::block_number();

			// make sure auction is started
			ensure!(block_number >= auction.start, Error::<T>::AuctionNotStarted);

			if let Some(ref current_bid) = auction.bid {
				ensure!(value > current_bid.1, Error::<T>::InvalidBidPrice);
			} else {
				ensure!(!value.is_zero(), Error::<T>::InvalidBidPrice);
			}
			let bid_result = T::Handler::on_new_bid(
				block_number,
				id,
				(from.clone(), value),
				auction.bid.clone(),
			);

			ensure!(bid_result.accept_bid, Error::<T>::BidNotAccepted);
			if let Some(new_end) = bid_result.auction_end {
				if let Some(old_end_block) = auction.end {
					<AuctionEndTime<T>>::remove(&old_end_block, id);
				}
				if let Some(new_end_block) = new_end {
					<AuctionEndTime<T>>::insert(&new_end_block, id, true);
				}
				auction.end = new_end;
			}
			auction.bid = Some((from.clone(), value));
			<Auctions<T>>::insert(id, auction);
			Self::deposit_event(RawEvent::Bid(id, from, value));
		}

		fn on_finalize(now: T::BlockNumber) {
			Self::_on_finalize(now);
		}
	}
}

decl_error! {
	/// Error for auction module.
	pub enum Error for Module<T: Trait> {
		AuctionNotExist,
		AuctionNotStarted,
		BidNotAccepted,
		InvalidBidPrice,
	}
}

impl<T: Trait> Module<T> {
	fn _on_finalize(now: T::BlockNumber) {
		for (auction_id, _) in <AuctionEndTime<T>>::drain(&now) {
			if let Some(auction) = <Auctions<T>>::take(&auction_id) {
				T::Handler::on_auction_ended(auction_id, auction.bid.clone());
			}
		}
	}
}

impl<T: Trait> Auction<T::AccountId, T::BlockNumber> for Module<T> {
	type AuctionId = T::AuctionId;
	type Balance = T::Balance;

	fn auction_info(id: Self::AuctionId) -> Option<AuctionInfo<T::AccountId, Self::Balance, T::BlockNumber>> {
		Self::auctions(id)
	}

	fn update_auction(
		id: Self::AuctionId,
		info: AuctionInfo<T::AccountId, Self::Balance, T::BlockNumber>,
	) -> DispatchResult {
		let auction = <Auctions<T>>::get(id).ok_or(Error::<T>::AuctionNotExist)?;
		if let Some(old_end) = auction.end {
			<AuctionEndTime<T>>::remove(&old_end, id);
		}
		if let Some(new_end) = info.end {
			<AuctionEndTime<T>>::insert(&new_end, id, true);
		}
		<Auctions<T>>::insert(id, info);
		Ok(())
	}

	fn new_auction(start: T::BlockNumber, end: Option<T::BlockNumber>) -> Self::AuctionId {
		let auction = AuctionInfo { bid: None, start, end };
		let auction_id = Self::auctions_index();
		<AuctionsIndex<T>>::mutate(|n| *n += Self::AuctionId::one());
		<Auctions<T>>::insert(auction_id, auction);
		if let Some(end_block) = end {
			<AuctionEndTime<T>>::insert(&end_block, auction_id, true);
		}

		auction_id
	}

	fn remove_auction(id: Self::AuctionId) {
		if let Some(auction) = <Auctions<T>>::take(&id) {
			if let Some(end_block) = auction.end {
				<AuctionEndTime<T>>::remove(&end_block, id);
			}
		}
	}
}
