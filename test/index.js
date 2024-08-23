const axios = require('axios')
const readlineSync = require('readline-sync');
const { createPublicClient, http, parseEther, createWalletClient, formatEther } = require('viem');
const { privateKeyToAccount } = require('viem/accounts');
const { anvil } = require('viem/chains')

const DEAD_ADDRESS = '0x000000000000000000000000000000000000dEaD'

async function main() {
  console.log('Welcome to Upnode signer-proxy integration test')

  const publicClient = createPublicClient({
    chain: anvil,
    transport: http()
  })

  const testWalletClient = createWalletClient({
    chain: anvil,
    transport: http(),
  })

  // test dummy account, don't use in the production!!!
  const testAccount = privateKeyToAccount('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80')

  let blockNumber, chainId;

  try {
    blockNumber = await publicClient.getBlockNumber()
    chainId = await publicClient.chain.id
  } catch (err) {
    console.error(err)
    console.log("Can't connect to the chain, please start anvil")
  }

  console.log('Chain', chainId, 'connected block number', blockNumber)

  const endpoint = readlineSync.question('Enter signer endpoint: ')

  console.log('Getting wallet address...')

  const addressResponse = await axios.get(`${endpoint}/address`);
  const address = addressResponse.data.address

  console.log('Wallet Address:', address)

  {
    // Transfer some fund to the wallet
    const hash = await testWalletClient.sendTransaction({
      account: testAccount,
      to: address,
      value: parseEther('0.0002'),
    })

    await publicClient.waitForTransactionReceipt({ hash })
  }

  // Get target balance before
  const balanceBefore = await publicClient.getBalance({
    address: DEAD_ADDRESS,
  })

  console.log('Balance Before:', formatEther(balanceBefore))

  const nonce = await publicClient.getTransactionCount({ address })

  const transaction = {
    from: address,
    to: DEAD_ADDRESS,
    value: parseEther('0.0001').toString(),
    gas: 21500,
    gasPrice: 1000000000,
    nonce,
    data: '0x123456',
    chainId: chainId,
  };

  const txResponse = await axios.post(endpoint, {
    jsonrpc: '2.0',
    method: 'eth_signTransaction',
    params: [transaction],
    id: 1,
  })
  const rawTx = txResponse.data.result
  
  console.log('Signed Tx:', rawTx)

  {
    // Submit signed transaction
    const hash = await publicClient.sendRawTransaction({
      serializedTransaction: rawTx,
    })
    
    console.log('Transaction submitted hash:', hash)
    setTimeout(() => console.log("If it has been taking long then there's something wrong..."), 5000)

    await publicClient.waitForTransactionReceipt({ hash })
    const transaction = await publicClient.getTransaction({ hash })

    console.log('Transaction confirmed')

    if (transaction.input != '0x123456') {
      console.error('!!! Transaction input is not matched !!!')
    }
  }

  // Get target balance before
  const balanceAfter = await publicClient.getBalance({
    address: DEAD_ADDRESS,
  })

  console.log('Balance After:', formatEther(balanceAfter))

  if (balanceAfter - balanceBefore === parseEther('0.0001')) {
    console.log('Test passed')
  } else {
    console.log('!!! Incorrect balance diff:', formatEther(balanceAfter - balanceBefore), '!!!')
  }
}

main().then(() => process.exit(0)).catch(err => {
  console.error(err)
  process.exit(1)
})
