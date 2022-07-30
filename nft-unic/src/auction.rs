use crate::*;
use near_sdk::{ext_contract, Gas};

const GAS_FOR_NFT_APPROVE: Gas = Gas(10_000_000_000_000);
const NO_DEPOSIT: Balance = 0;

pub trait NonFungibleTokenCore {
    //create auction for given token id
    fn approve_nft_auction(
        &mut self,
        auction_token: TokenId,
        account_id: AccountId,
        start_time: u64,
        end_time: u64,
        msg: Option<String>,
    );

    fn auction_is_approved(
        &self,
        token_id: TokenId,
        auction_by_owner: AccountId,
        auction_id: Option<u64>,
    ) -> bool;

    fn nft_auction_revoke(&mut self, token_id: TokenId, account_id: AccountId);

    fn nft_revoke_all_auctions(&mut self, token_id: TokenId);
}

#[ext_contract(ext_nft_auction_receiver)]
trait NftAuctionReceiver {
    fn on_create_auction(
        &mut self,
        auction_token: TokenId,
        account_id: AccountId,
        auction_id: u64,
        start_time: u64,
        end_time: u64,
        msg: String,
    );
}

#[near_bindgen]
impl NonFungibleTokenCore for Contract {
    //creates auction and sets start price
    #[payable]
    fn approve_nft_auction(
        &mut self,
        auction_token: TokenId,
        account_id: AccountId,
        start_time: u64,
        end_time: u64,
        msg: Option<String>,
    ) {
        assert_at_least_one_yocto();

        let mut token = self.tokens_by_id.get(&auction_token).expect("No token");

        assert_eq!(
            &env::predecessor_account_id(),
            &token.owner_id,
            "Predecessor must be the token owner."
        );
        /*assert_eq!(
            self.auctioned_tokens.contains(&auction_token),
            false,
            "Already auctioned"
        ); */

        let auction_id: u64 = token.auction_list_id;

        let is_new_auction = token
            .auctions_by_owner
            .insert(account_id.clone(), auction_id)
            .is_none();

        let storage_used = if is_new_auction {
            bytes_for_approved_account_id(&account_id)
        } else {
            0
        };

        token.auction_list_id += 1;
        self.tokens_by_id.insert(&auction_token, &token);

        refund_deposit(storage_used);

        if let Some(msg) = msg {
            ext_nft_auction_receiver::on_create_auction(
                auction_token,
                token.owner_id,
                auction_id,
                start_time,
                end_time,
                msg,
                account_id,
                NO_DEPOSIT,
                env::prepaid_gas() - GAS_FOR_NFT_APPROVE,
            )
            .as_return(); // Returning this promise
        }
    }

    //check if the passed in account has access to approve auction of the token ID
    fn auction_is_approved(
        &self,
        token_id: TokenId,
        auction_by_owner: AccountId,
        auction_id: Option<u64>,
    ) -> bool {
        let token = self.tokens_by_id.get(&token_id).expect("No token");

        //get the approval number for the passed in account ID
        let approval = token.auctions_by_owner.get(&auction_by_owner);

        //if there was some approval ID found for the account ID
        if let Some(approval) = approval {
            //if a specific approval_id was passed into the function
            if let Some(auction_id) = auction_id {
                //return if the approval ID passed in matches the actual approval ID for the account
                auction_id == *approval
                //if there was no approval_id passed into the function, we simply return true
            } else {
                true
            }
            //if there was no approval ID found for the account ID, we simply return false
        } else {
            false
        }
    }

    #[payable]
    fn nft_auction_revoke(&mut self, token_id: TokenId, account_id: AccountId) {
        assert_one_yocto();
        //get the token object using the passed in token_id
        let mut token = self.tokens_by_id.get(&token_id).expect("No token");

        //get the caller of the function and assert that they are the owner of the token
        let predecessor_account_id = env::predecessor_account_id();
        assert_eq!(&predecessor_account_id, &token.owner_id);

        //if the account ID was in the token's approval, we remove it and the if statement logic executes
        if token.auctions_by_owner.remove(&account_id).is_some() {
            //refund the funds released by removing the approved_account_id to the caller of the function
            refund_approved_auction_account_ids_iter(predecessor_account_id, [account_id].iter());

            //insert the token back into the tokens_by_id collection with the account_id removed from the auction approval list
            self.tokens_by_id.insert(&token_id, &token);
        }
    }

    //revoke all accounts from transferring the token on your behalf
    #[payable]
    fn nft_revoke_all_auctions(&mut self, token_id: TokenId) {
        assert_one_yocto();

        //get the token object from the passed in token ID
        let mut token = self.tokens_by_id.get(&token_id).expect("No token");
        //get the caller and make sure they are the owner of the tokens
        let predecessor_account_id = env::predecessor_account_id();
        assert_eq!(&predecessor_account_id, &token.owner_id);

        //only revoke if the approved account IDs for the token is not empty
        if !token.auctions_by_owner.is_empty() {
            //refund the approved auction account IDs to the caller of the function
            refund_approved_auction_account_ids(predecessor_account_id, &token.auctions_by_owner);
            //clear the approved auction account IDs
            token.auctions_by_owner.clear();
            //insert the token back into the tokens_by_id collection with the approved account IDs cleared
            self.tokens_by_id.insert(&token_id, &token);
        }
    }
}
