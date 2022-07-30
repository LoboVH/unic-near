use crate::*;
use near_sdk::promise_result_as_success;

//struct that holds important information about each sale on the market
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Auction {
    //owner of the sale
    pub owner_id: AccountId,
    //market contract's approval ID to transfer the token on behalf of the owner
    pub auction_id: u64,
    //nft contract where the token was minted
    pub nft_contract_id: String,
    //actual token ID for sale
    pub auction_token: String,
    //sale price in yoctoNEAR that the token is listed for
    pub sale_conditions: SalePriceInYoctoNear,
    pub start_time: u128,
    pub end_time: u128,
    pub winner: Option<AccountId>,
    pub is_near_claimed: bool,
    pub is_nft_claimed: bool,
}

#[near_bindgen]
impl Contract {
    //removes a auction from the market.
    #[payable]
    pub fn remove_auction(&mut self, nft_contract_id: AccountId, token_id: String) {
        //assert that the user has attached exactly 1 yoctoNEAR (for security reasons)
        assert_one_yocto();

        let contract_id: AccountId = nft_contract_id.into();
        let contract_and_auction_token_id = format!("{}{}{}", contract_id, DELIMETER, token_id);

        let auction = self
            .auctions
            .get(&contract_and_auction_token_id)
            .expect("No Auction");

        if !(auction.winner == None) {
            let auctionWinnerId = auction.winner.map(|a| a.into()).unwrap();
            let old_winner = Promise::new(auctionWinnerId);
            old_winner.transfer(auction.sale_conditions.0 - ENROLL_FEE);
        }

        let auction = self.internal_remove_auction(contract_id, token_id.clone());
        //get the predecessor of the call and make sure they're the owner of the sale
        let owner_id = env::predecessor_account_id();
        //if this fails, the remove auction will revert
        assert_eq!(owner_id, auction.owner_id, "Must be auction owner");
    }

    #[payable]
    pub fn offer_bid(&mut self, nft_contract_id: AccountId, token_id: String) {
        //create the unique auction ID from the nft contract and token
        let contract_id: AccountId = nft_contract_id.into();
        let contract_and_auction_token_id = format!("{}{}{}", contract_id, DELIMETER, token_id);

        let mut auction = self
            .auctions
            .get(&contract_and_auction_token_id)
            .expect("No Auction");

        assert_eq!(
            env::block_timestamp() > (auction.start_time) as u64,
            true,
            "This auction has not started"
        );
        assert_eq!(
            env::block_timestamp() < (auction.end_time) as u64,
            true,
            "This auction is already done"
        );
        assert_eq!(
            env::attached_deposit() > auction.sale_conditions.0,
            true,
            "Price must be greater than current winner's price"
        );

        //get the buyer ID which is the person who called the function and make sure they're not the owner of the auction.
        let buyer_id = env::predecessor_account_id();
        assert_ne!(
            auction.owner_id, buyer_id,
            "Cannot bid on your own auction."
        );

        if !(auction.winner == None) {
            let auctionWinnerId = auction.winner.map(|a| a.into()).unwrap();
            let old_winner = Promise::new(auctionWinnerId);
            old_winner.transfer(auction.sale_conditions.0 - ENROLL_FEE);
        }
        auction.winner = Some(env::predecessor_account_id());
        auction.sale_conditions.0 = env::attached_deposit();
        self.auctions
            .insert(&contract_and_auction_token_id, &auction);
    }

    #[payable]
    pub fn process_auction_purchase(
        &mut self,
        nft_contract_id: AccountId,
        token_id: String,
    ) -> Promise {
        let deposit = env::attached_deposit();
        let contract_id: AccountId = nft_contract_id.into();
        let contract_and_auction_token_id = format!("{}{}{}", contract_id, DELIMETER, token_id);

        let auction = self
            .auctions
            .get(&contract_and_auction_token_id)
            .expect("No Auction");

        let buyer_id: AccountId = auction.winner.map(|a| a.into()).unwrap();

        let price = auction.sale_conditions;

        assert_eq!(
            env::block_timestamp() > (auction.end_time) as u64,
            true,
            "The auction is not over yet"
        );

        assert_eq!(auction.is_nft_claimed, false, "NFT is already claimed..!!!");

        assert_eq!(auction.is_near_claimed, false, "NEAR already claimed N");

        assert!(
            deposit >= auction.sale_conditions.0,
            "Attached deposit must be greater than or equal to the current price: {:?}",
            price
        );

        //get the auction object by removing the auction
        let mut auction = self.internal_remove_auction(contract_id.clone(), token_id.clone());

        ext_contract::nft_transfer_payout(
            buyer_id.clone(),                 //purchaser (person to transfer the NFT to)
            token_id,                         //token ID to transfer
            auction.auction_id, //market contract's approval ID in order to transfer the token on behalf of the owner
            "payout from market".to_string(), //memo (to include some context)
            /*
                the price that the token was purchased for. This will be used in conjunction with the royalty percentages
                for the token in order to determine how much money should go to which account.
            */
            price,
            10,
            contract_id,
            1,
            GAS_FOR_NFT_TRANSFER, //the maximum amount of accounts the market can payout at once (this is limited by GAS)
        )
        //after the transfer payout has been initiated, we resolve the promise by calling our own resolve_purchase function.
        //resolve purchase will take the payout object returned from the nft_transfer_payout and actually pay the accounts
        .then(ext_self::resolve_auction_purchase(
            buyer_id,
            price,
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_RESOLVE_PURCHASE,
        ))
    }

    #[private]
    pub fn resolve_auction_purchase(&mut self, buyer_id: AccountId, price: U128) -> U128 {
        // checking for payout information returned from the nft_transfer_payout method
        let payout_option = promise_result_as_success().and_then(|value| {
            //if we set the payout_option to None, that means something went wrong and we should refund the buyer
            near_sdk::serde_json::from_slice::<Payout>(&value)
                //converts the result to an optional value
                .ok()
                //returns None if the none. Otherwise executes the following logic
                .and_then(|payout_object| {
                    //we'll check if length of the payout object is > 10 or it's empty. In either case, we return None
                    if payout_object.payout.len() > 10 || payout_object.payout.is_empty() {
                        env::log_str("Cannot have more than 10 royalties");
                        None

                    //if the payout object is the correct length, we move forward
                    } else {
                        //we'll keep track of how much the nft contract wants us to payout. Starting at the full price payed by the buyer
                        let mut remainder = price.0;

                        //loop through the payout and subtract the values from the remainder.
                        for &value in payout_object.payout.values() {
                            //checked sub checks for overflow or any errors and returns None if there are problems
                            remainder = remainder.checked_sub(value.0)?;
                        }
                        //Check to see if the NFT contract sent back a faulty payout that requires us to pay more or too little.
                        //The remainder will be 0 if the payout summed to the total price. The remainder will be 1 if the royalties
                        //we something like 3333 + 3333 + 3333.
                        if remainder == 0 || remainder == 1 {
                            //set the payout_option to be the payout because nothing went wrong
                            Some(payout_object.payout)
                        } else {
                            //if the remainder was anything but 1 or 0, we return None
                            None
                        }
                    }
                })
        });

        // if the payout option was some payout, we set this payout variable equal to that some payout
        let payout = if let Some(payout_option) = payout_option {
            payout_option
        //if the payout option was None, we refund the buyer for the price they payed and return
        } else {
            Promise::new(buyer_id).transfer(u128::from(price));
            // leave function and return the price that was refunded
            return price;
        };

        // NEAR payouts
        for (receiver_id, amount) in payout {
            Promise::new(receiver_id).transfer(amount.0);
        }

        //return the price payout out
        price
    }
}

//this is the cross contract call that we call on our own contract.
/*
    private method used to resolve the promise when calling nft_transfer_payout. This will take the payout object and
    check to see if it's authentic and there's no problems. If everything is fine, it will pay the accounts. If there's a problem,
    it will refund the buyer for the price.
*/
#[ext_contract(ext_self)]
trait ExtSelf {
    fn resolve_auction_purchase(&mut self, buyer_id: AccountId, price: U128) -> Promise;
}
