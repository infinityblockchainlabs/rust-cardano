extern crate serde_json;

use cardano::address;
use cardano::bip;
use cardano::hdwallet;
use cardano::wallet::bip44;
use cardano::wallet::scheme::Wallet;

use cardano::cbor_event;
use cardano::{config::ProtocolMagic, fee, txutils, tx, coin};
use cardano::util::base58;
use cardano::address::ExtendedAddr;
use cardano::tx::{txaux_serialize};

use std::os::raw::c_char;
use std::{ffi, ptr, slice};

use types::{AccountPtr, WalletPtr};
use address::ffi_address_to_base58;

use serde_json::{Value, Error, error::ErrorCode};

const PROTOCOL_MAGIC : u32 = 764824073;
const DEBUG: bool = false;

/* ******************************************************************************* *
 *                                  Wallet object                                  *
 * ******************************************************************************* */

// TODO: one of the major missing element is a proper clean error handling

/// Create a HD BIP44 compliant Wallet from the given entropy and a password
///
/// Password can be empty
///
/// use the function `cardano_wallet_delete` to free all the memory associated to the returned
/// object. This function may fail if:
///
/// - panic: if there is no more memory to allocate the object to return
/// - panic or return 0 (nullptr or NULL) if the given seed_ptr is of invalid length
///
#[no_mangle]
pub extern "C" fn cardano_wallet_new(
    entropy_ptr: *const u8, /* expecting entropy ptr ... */
    entropy_size: usize,    /* entropy size */
    password_ptr: *const u8, /* password ptr */
    password_size: usize,   /* password size */
) -> WalletPtr {
    let entropy_slice = unsafe { slice::from_raw_parts(entropy_ptr, entropy_size) };
    let password = unsafe { slice::from_raw_parts(password_ptr, password_size) };

    let entropy = match bip::bip39::Entropy::from_slice(entropy_slice) {
        Err(_) => return ptr::null_mut(),
        Ok(e) => e,
    };

    let wallet = bip44::Wallet::from_entropy(&entropy, &password, hdwallet::DerivationScheme::V2);

    let wallet_box = Box::new(wallet);
    Box::into_raw(wallet_box)
}

/// take ownership of the given pointer and free the associated data
///
/// The data must be a valid Wallet created by `cardano_wallet_new`.
#[no_mangle]
pub extern "C" fn cardano_wallet_delete(wallet_ptr: WalletPtr) {
    unsafe { Box::from_raw(wallet_ptr) };
}

/* ******************************************************************************* *
 *                                 Account object                                  *
 * ******************************************************************************* */

/// create a new account, the account is given an alias and an index,
/// the index is the derivation index, we do not check if there is already
/// an account with this given index. The alias here is only an handy tool
/// to retrieve a created account from a wallet.
///
/// The returned object is not owned by any smart pointer or garbage collector.
/// To avoid memory leak, use `cardano_account_delete`
///
#[no_mangle]
pub extern "C" fn cardano_account_create(
    wallet_ptr: WalletPtr,
    account_alias: *mut c_char,
    account_index: u32,
) -> AccountPtr {
    let wallet = unsafe { wallet_ptr.as_mut() }.expect("Not a NULL PTR");
    let account_alias = unsafe { ffi::CStr::from_ptr(account_alias).to_string_lossy() };

    let account = wallet.create_account(&account_alias, account_index);
    let account = Box::new(account.public());

    Box::into_raw(account)
}

/// take ownership of the given pointer and free the memory associated
#[no_mangle]
pub extern "C" fn cardano_account_delete(account_ptr: AccountPtr) {
    unsafe { Box::from_raw(account_ptr) };
}

#[no_mangle]
pub extern "C" fn cardano_account_generate_addresses(
    account_ptr: AccountPtr,
    internal: bool,
    from_index: u32,
    num_indices: usize,
    addresses_ptr: *mut *mut c_char,
) -> usize {
    let account = unsafe { account_ptr.as_mut() }.expect("Not a NULL PTR");

    let addr_type = if internal {
        bip44::AddrType::Internal
    } else {
        bip44::AddrType::External
    };

    account
        .address_generator(addr_type, from_index)
        .expect("we expect the derivation to happen successfully")
        .take(num_indices)
        .enumerate()
        .map(|(idx, xpub)| {
            let address = address::ExtendedAddr::new_simple(*xpub.unwrap());
            let c_address = ffi_address_to_base58(&address);
            // make sure the ptr is stored at the right place with alignments and all
            unsafe {
                ptr::write(
                    addresses_ptr.wrapping_offset(idx as isize),
                    c_address.into_raw(),
                )
            };
        })
        .count()
}


#[derive(Debug)]
struct Transaction {
    txaux   : tx::TxAux,
    fee     : fee::Fee,
    txid    : *mut c_char
}

fn cardano_new_transaction  ( wallet_ptr: WalletPtr
                            , utxos     : *const c_char
                            , from_addr : *const c_char
                            , to_addrs  : *const c_char )
-> Result<Transaction, Error> 
{
    // parse input c_char to string
    let utxos = unsafe { ffi::CStr::from_ptr(utxos) };
    let addrs = unsafe { ffi::CStr::from_ptr(to_addrs) };

    let utxos_str = utxos.to_str().unwrap();
    let addrs_str = addrs.to_str().unwrap();

    // Parse the string of data into json
    let utxos_json: Value = serde_json::from_str(&utxos_str.to_string())?;
    let addrs_json: Value = serde_json::from_str(&addrs_str.to_string())?;

    if !utxos_json.is_array() || !addrs_json.is_array() {
        return Err(Error::syntax(ErrorCode::ExpectedObjectOrArray, 1, 1));
    }

    // get input array length
    let utxos_arr_len = utxos_json.as_array().unwrap().len();
    let addrs_arr_len = addrs_json.as_array().unwrap().len();

    if utxos_arr_len <= 0 || addrs_arr_len <= 0 {
        return Err(Error::syntax(ErrorCode::ExpectedObjectOrArray, 1, 1));
    }

    // the wallet created from_addr
    let wallet = unsafe { wallet_ptr.as_mut() }.expect("Not a NULL PTR");

    // init input & output of transaction
    let mut inputs = vec![];
    let mut outputs = vec![];

    // convert from_addr from string to ExtendedAddr 
    let from_addr = unsafe {
        ffi::CStr::from_ptr(from_addr).to_string_lossy()
    };

    let from_addr_bytes = base58::decode_bytes(from_addr.as_bytes()).unwrap();
    let from = ExtendedAddr::from_bytes(&from_addr_bytes[..]).unwrap();

    // init transaction input from utxos
    for x in 0..utxos_arr_len {
        let trx_id = &utxos_json[x]["id"].as_str().unwrap();        
        let txin = tx::TxIn::new(tx::TxId::from_slice(&hex::decode(trx_id).unwrap()).unwrap(), utxos_json[x]["index"].to_string().parse::<u32>().unwrap());
        
        let addressing = bip44::Addressing::new(0, bip44::AddrType::External, 0).unwrap();
        let txout = tx::TxOut::new(from.clone(), coin::Coin::new(utxos_json[x]["value"].to_string().parse::<u64>().unwrap()).unwrap());

        inputs.push(txutils::Input::new(txin, txout, addressing));
    }

    // init transaction output from to_address
    for x in 0..addrs_arr_len {
        let to_raw = base58::decode_bytes(addrs_json[x]["addr"].as_str().unwrap().as_bytes()).unwrap();
        let to = ExtendedAddr::from_bytes(&to_raw[..]).unwrap();

        outputs.push(tx::TxOut::new(to.clone(), coin::Coin::new(addrs_json[x]["value"].to_string().parse::<u64>().unwrap()).unwrap()))
    }

    let (txaux, fee) = wallet.new_transaction(
        ProtocolMagic::new(PROTOCOL_MAGIC),
        fee::SelectionPolicy::default(),
        inputs.iter(),
        outputs,
        &txutils::OutputPolicy::One(from.clone())).unwrap();

    if DEBUG {
        println!("############## Transaction prepared #############");
        println!("  txaux {}", txaux);
        println!("  tx id {}", txaux.tx.id());
        println!("  from address {}", from);
        println!("  fee: {:?}", fee);
        println!("###################### End ######################");
    }

    let txid = format!("{}", txaux.tx.id());

    delete_wallet(wallet_ptr);
    return Ok(Transaction {
        txaux   : txaux,
        fee     : fee,
        txid    : ffi::CString::new(txid).unwrap().into_raw()
    })
}
