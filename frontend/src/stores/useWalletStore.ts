// hooks/useWalletStore.ts

import { create } from 'zustand';
import { persist, subscribeWithSelector } from 'zustand/middleware';
import { Connection, PublicKey, Transaction, VersionedTransaction } from '@solana/web3.js';
import { WalletAdapterNetwork } from '@solana/wallet-adapter-base';
import { WalletContextState, useWallet } from '@solana/wallet-adapter-react';
import { createSelectors } from '@/lib/store';
import { solanaRPC, defaultNetwork } from '@/config';
import { notify } from '@/lib/notifications';
import { captureException } from '@/lib/monitoring';

interface WalletStoreState {
  connection: Connection;
  network: WalletAdapterNetwork;
  publicKey: PublicKey | null;
  wallets: WalletContextState['wallets'];
  connecting: boolean;
  disconnecting: boolean;
  balance: number;
  recentTransactions: string[];
  error: Error | null;
  autoConnect: boolean;
  lastActivity: number;
}

interface WalletStoreActions {
  connect: (wallet?: WalletContextState) => Promise<void>;
  disconnect: () => Promise<void>;
  signMessage: (message: Uint8Array) => Promise<Uint8Array | null>;
  sendTransaction: (
    transaction: Transaction | VersionedTransaction,
    options?: { signers?: any[]; skipPreflight?: boolean }
  ) => Promise<string>;
  switchNetwork: (network: WalletAdapterNetwork) => Promise<void>;
  refreshBalance: () => Promise<void>;
  reset: () => void;
}

const initialState: WalletStoreState = {
  connection: new Connection(solanaRPC),
  network: defaultNetwork,
  publicKey: null,
  wallets: [],
  connecting: false,
  disconnecting: false,
  balance: 0,
  recentTransactions: [],
  error: null,
  autoConnect: false,
  lastActivity: 0,
};

export const useWalletStore = createSelectors(
  create<WalletStoreState & WalletStoreActions>()(
    persist(
      subscribeWithSelector((set, get) => ({
        ...initialState,

        connect: async (wallet) => {
          try {
            set({ connecting: true, error: null });
            
            const connection = new Connection(solanaRPC);
            const publicKey = wallet?.publicKey || null;

            if (!publicKey) {
              throw new Error('Wallet connection failed');
            }

            const balance = await connection.getBalance(publicKey);

            set({
              connection,
              publicKey,
              balance: balance / 1e9,
              connecting: false,
              lastActivity: Date.now(),
            });

            notify.success('Wallet connected successfully');
          } catch (error) {
            set({ connecting: false, error: error as Error });
            captureException(error);
            notify.error('Wallet connection failed');
            throw error;
          }
        },

        disconnect: async () => {
          try {
            set({ disconnecting: true });
            const { connection, publicKey } = get();

            if (publicKey) {
              await connection.request({ method: 'disconnect' });
            }

            set({
              ...initialState,
              connection: new Connection(solanaRPC),
              disconnecting: false,
            });

            notify.info('Wallet disconnected');
          } catch (error) {
            set({ disconnecting: false, error: error as Error });
            captureException(error);
            throw error;
          }
        },

        signMessage: async (message) => {
          const { publicKey, connection } = get();
          const wallet = useWallet();

          try {
            if (!publicKey || !wallet.signMessage) {
              throw new Error('Wallet not ready for signing');
            }

            const signature = await wallet.signMessage(message);
            set({ lastActivity: Date.now() });
            
            return signature;
          } catch (error) {
            set({ error: error as Error });
            captureException(error);
            throw error;
          }
        },

        sendTransaction: async (transaction, options = {}) => {
          const { connection, publicKey } = get();
          const wallet = useWallet();

          try {
            if (!publicKey || !wallet.sendTransaction) {
              throw new Error('Wallet not ready for transactions');
            }

            const {
              signature,
              context: { slot },
            } = await wallet.sendTransaction(transaction, connection, options);

            await connection.confirmTransaction({
              signature,
              abortSignal: AbortSignal.timeout(30000),
              minContextSlot: slot,
            });

            set(state => ({
              recentTransactions: [signature, ...state.recentTransactions.slice(0, 9)],
              lastActivity: Date.now(),
            }));

            return signature;
          } catch (error) {
            set({ error: error as Error });
            captureException(error);
            throw error;
          }
        },

        switchNetwork: async (network) => {
          try {
            const newConnection = new Connection(
              network === WalletAdapterNetwork.Devnet 
                ? solanaRPC.devnet 
                : solanaRPC.mainnet
            );

            const balance = get().publicKey 
              ? await newConnection.getBalance(get().publicKey!) / 1e9
              : 0;

            set({
              network,
              connection: newConnection,
              balance,
              lastActivity: Date.now(),
            });

            notify.info(`Switched to ${network} network`);
          } catch (error) {
            set({ error: error as Error });
            captureException(error);
            throw error;
          }
        },

        refreshBalance: async () => {
          const { connection, publicKey } = get();
          if (!publicKey) return;

          try {
            const balance = await connection.getBalance(publicKey);
            set({ balance: balance / 1e9 });
          } catch (error) {
            set({ error: error as Error });
            captureException(error);
          }
        },

        reset: () => set(initialState),
      })),
      {
        name: 'wallet-storage',
        partialize: (state) => 
          Object.fromEntries(
            Object.entries(state).filter(([key]) => 
              !['connecting', 'disconnecting', 'error'].includes(key)
            )
          ),
      }
    )
  )
);

// Utility hooks
export const useWalletBalance = () => 
  useWalletStore.use.balance();

export const useRecentTransactions = () => 
  useWalletStore.use.recentTransactions();

export const useNetworkStatus = () => 
  useWalletStore.use.network();

// Type extensions
declare module '@/types' {
  interface WalletState extends WalletStoreState, WalletStoreActions {}
}
