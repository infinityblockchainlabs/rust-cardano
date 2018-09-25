#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include "cardano.h"

static const uint8_t static_wallet_entropy[16] = { 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15 };

int wallet_test(void) {
	static const char* alias = "Test Wallet";
	static char *address;

	cardano_wallet *wallet = cardano_wallet_new(static_wallet_entropy, 16, "abc", 3);
	if (!wallet) goto error;

	cardano_account *account = cardano_account_create(wallet, alias, 0);
	if (!account) goto error;

	cardano_account_generate_addresses(account, 0, 0, 1, &address);

	printf("address generated: %s\n", address);

	printf("address is valid: %s\n", cardano_address_is_valid(address) ? "NO" : "YES");

	cardano_account_delete(account);

	cardano_wallet_delete(wallet);

	// Transaction test
	static const char* root_key = "d8a7234357dcfc003c99ac410262de9bf2b43c1886939045012d270de6cb2f4360453ef620552718dece81b9b0efcc55a71a5b9a417ccf772fe90ec857c2f0da77794c3cacfc998a9f2ad30f17b6370a9a56695aa35ea702e7abd430d7615637";
	static const char* txid = "678f01893645b40557166a52637e6a5db048d34f09da6096a7ceb63abfeb5187";

	cardano_xprv *xprv = cardano_xprv_from_bytes(root_key);
	cardano_xpub *to_addr = cardano_xprv_to_xpub(xprv);
	cardano_xpub *change_addr = cardano_xprv_to_xpub(xprv);

	cardano_txoptr *txoptr = cardano_transaction_output_ptr_new(txid, 0);
	if (!txoptr) goto error;

	cardano_txoutput *txoutput = cardano_transaction_output_new(to_addr, 1000000);
	if (!txoutput) goto error;

	cardano_transaction_builder *builder = cardano_transaction_builder_new();
	if (!builder) goto error;

	cardano_transaction_builder_add_output(builder, txoutput);
	cardano_transaction_builder_add_input(builder, txoptr, 600000);
	cardano_transaction_builder_add_change_addr(builder, change_addr);


	return 0;
error:
	return -1;
}

int main(int argc, char* argv[]) {
	if (wallet_test()) exit(35);
	return 0;
}
