use std::ops::Index;
use std::path::Path;
use monero_db::MoneroDB;
use monero_serai::{block::Block, transaction::Transaction};

fn main() {
    let path = Path::new("");
    let db = MoneroDB::open(path, true).unwrap();
    let height = db.get_blockchain_height().unwrap();

    for height in 0..height {
        let block = db.get_block(height).unwrap();
        let serai_block = Block::read(&mut block.as_slice()).unwrap();

        match db.get_block_height(&serai_block.hash().into()) {
            Ok(got_height) => if got_height.block_height != height {
                panic!("DB height incorrect: {}", height);
            }
            Err(e) => panic!("Could not find block height in DB: {}", height)
        }

        if serai_block.serialize() != block {
            panic!("Could not serialize block: {}", height);
        }

        for tx in serai_block.txs {
            let tx_id = db.get_tx_indices(&tx.into()).unwrap().tx_id;
            let tx_bytes = {
                let mut pruned = db.get_tx_pruned(tx_id).unwrap();
                pruned.extend_from_slice(&db.get_tx_prunable(tx_id).unwrap());
                pruned
            };

            let serai_tx = Transaction::read(&mut tx_bytes.as_slice()).unwrap();
            if serai_tx.hash() != tx {
                panic!("Got incorrect tx hash: {}", hex::encode(tx));
            }
            if serai_tx.serialize() != tx_bytes {
                panic!("incorrect serialisation tx hash: {}", hex::encode(tx));
            }

        }

        if height % 10_000 == 0 {
            println!("scanned: 0..{}", height);
        }


    }
}
