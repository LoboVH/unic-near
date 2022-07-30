use crate::*;

/// approval callbacks from NFT Contracts

//struct for keeping track of the sale conditions for a Sale
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SaleArgs {
    pub sale_conditions: SalePriceInYoctoNear,
}

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
impl NftAuctionReceiver for Contract {
    fn on_create_auction(
        &mut self,
        auction_token: TokenId,
        account_id: AccountId,
        auction_id: u64,
        start_time: u64,
        end_time: u64,
        msg: String,
    ) {
        let nft_contract_id = env::predecessor_account_id();
        //get the signer which is the person who initiated the transaction
        let signer_id = env::signer_account_id();

        assert_ne!(
            nft_contract_id, signer_id,
            "nft_on_approve should only be called via cross-contract call"
        );

        assert_eq!(account_id, signer_id, "owner_id should be signer_id");

        let storage_amount = self.storage_minimum_balance().0;
        //get the total storage paid by the owner
        let owner_paid_storage = self.storage_deposits.get(&signer_id).unwrap_or(0);

        let signer_storage_required =
            (self.get_supply_by_owner_id(signer_id).0 + 1) as u128 * storage_amount;

        assert!(
            owner_paid_storage >= signer_storage_required,
            "Insufficient storage paid: {}, for {} sales at {} rate of per sale",
            owner_paid_storage,
            signer_storage_required / STORAGE_PER_SALE,
            STORAGE_PER_SALE
        );

        let SaleArgs { sale_conditions } =
            //the sale conditions come from the msg field. The market assumes that the user passed
            //in a proper msg. If they didn't, it panics. 
            near_sdk::serde_json::from_str(&msg).expect("Not valid SaleArgs");

        let contract_and_auction_token_id =
            format!("{}{}{}", nft_contract_id, DELIMETER, auction_token);

        self.auctions.insert(
            &contract_and_auction_token_id,
            &Auction {
                owner_id: account_id.clone(),                 //owner of the sale / token
                auction_id, //approval ID for that token that was given to the market
                nft_contract_id: nft_contract_id.to_string(), //NFT contract the token was minted on
                auction_token: auction_token.clone(), //the actual token ID
                sale_conditions, //the sale conditions
                start_time: (start_time as u128) * (1_000_000_000 as u128),
                end_time: (end_time as u128) * (1_000_000_000 as u128),
                winner: None,
                is_near_claimed: false,
                is_nft_claimed: false,
            },
        );

        //get the auctions by owner ID for the given owner. If there are none, we create a new empty set
        let mut by_auction_owner_id =
            self.by_auction_owner_id
                .get(&account_id)
                .unwrap_or_else(|| {
                    UnorderedSet::new(
                        StorageKey::ByAuctionOwnerIdInner {
                            //we get a new unique prefix for the collection by hashing the owner account
                            account_id_hash: hash_account_id(&account_id),
                        }
                        .try_to_vec()
                        .unwrap(),
                    )
                });

        by_auction_owner_id.insert(&contract_and_auction_token_id);

        self.by_auction_owner_id
            .insert(&account_id, &by_auction_owner_id);

        //get the auction token IDs for the given nft contract ID. If there are none, we create a new empty set
        let mut auctions_by_nft_contract_id = self
            .auctions_by_nft_contract_id
            .get(&nft_contract_id)
            .unwrap_or_else(|| {
                UnorderedSet::new(
                    StorageKey::AuctionsByNFTContractIdInner {
                        //we get a new unique prefix for the collection by hashing the owner
                        account_id_hash: hash_account_id(&nft_contract_id),
                    }
                    .try_to_vec()
                    .unwrap(),
                )
            });

        //insert the token ID into the set
        auctions_by_nft_contract_id.insert(&auction_token);

        self.auctions_by_nft_contract_id
            .insert(&nft_contract_id, &auctions_by_nft_contract_id);
    }
}
