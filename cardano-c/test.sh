#!/bin/sh

if [ -d target/debug ]; then
	PROJECT_ROOT="./"
elif [ -d ../target/debug ]; then
	PROJECT_ROOT="../"
else
	echo "no target debug directory"
	exit 1
fi


C_ROOT="./"
C_LIB_A="./dist/cardano-c/x86_64-unknown-linux-gnu/debug/libcardano_c.a"

if [ ! -f "${C_LIB_A}" ]; then
	echo "no library file found. compile cardano-c first"
	exit 2
fi

gcc -o test-cardano-c.$$ -I "${C_ROOT}" "${C_ROOT}test/test.c" "${C_LIB_A}" -lpthread -lm -ldl
echo "######################################################################"
./test-cardano-c.$$
echo ""
echo "######################################################################"
rm test-cardano-c.$$
