// hooks/useMarketStore.ts

import { create } from 'zustand';
import { persist, subscribeWithSelector } from 'zustand/middleware';
import { Connection, PublicKey } from '@solana/web3.js';
import { WalletContextState } from '@solana/wallet-adapter-react';
import { produce } from 'immer';
import { BN } from '@project-serum/anchor';
import { MarketState, OrderBook, TradePair, AI ModelListing } from '@/types/market';
import { createSelectors } from '@/lib/store';
import { solanaRPC } from '@/config';
import { fetchMarketData, submitOrder } from '@/api/market';
import { notify } from '@/lib/notifications';
import { captureException } from '@/lib/monitoring';

interface MarketStoreState {
  connection: Connection;
  wallet: WalletContextState | null;
  baseCurrency: string;
  tradePairs: TradePair[];
  orderBooks: Record<string, OrderBook>;
  modelListings: AIModelListing[];
  recentTrades: Record<string, Array<{ price: number; quantity: number }>>;
  liquidityPools: Array<{
    pair: string;
    baseReserve: number;
    quoteReserve: number;
  }>;
  status: 'idle' | 'loading' | 'error';
  lastUpdated: number;
  error: Error | null;
}

interface MarketStoreActions {
  initialize: (wallet: WalletContextState) => Promise<void>;
  refreshMarketData: () => Promise<void>;
  addOrder: (
    pair: string,
    order: { price: number; quantity: number; side: 'buy' | 'sell' }
  ) => Promise<void>;
  cancelOrder: (orderId: string) => Promise<void>;
  updateLiquidity: (pair: string, baseDelta: number, quoteDelta: number) => void;
  setBaseCurrency: (currency: string) => void;
  subscribeToPair: (pair: string) => () => void;
  reset: () => void;
}

const initialState: MarketStoreState = {
  connection: new Connection(solanaRPC),
  wallet: null,
  baseCurrency: 'SOL',
  tradePairs: [],
  orderBooks: {},
  modelListings: [],
  recentTrades: {},
  liquidityPools: [],
  status: 'idle',
  lastUpdated: Date.now(),
  error: null,
};

export const useMarketStore = createSelectors(
  create<MarketStoreState & MarketStoreActions>()(
    persist(
      subscribeWithSelector((set, get) => ({
        ...initialState,

        initialize: async (wallet) => {
          try {
            set({ status: 'loading' });
            const connection = new Connection(solanaRPC);
            const [pairs, listings] = await Promise.all([
              fetchMarketData<TradePair[]>('/pairs'),
              fetchMarketData<AIModelListing[]>('/models'),
            ]);

            set({
              connection,
              wallet,
              tradePairs: pairs,
              modelListings: listings,
              status: 'idle',
              lastUpdated: Date.now(),
            });
          } catch (error) {
            set({ status: 'error', error: error as Error });
            captureException(error);
          }
        },

        refreshMarketData: async () => {
          const { connection, tradePairs } = get();
          try {
            set({ status: 'loading' });
            
            const [updatedPairs, orderBooks] = await Promise.all([
              fetchMarketData<TradePair[]>('/pairs'),
              Promise.all(
                tradePairs.map(pair =>
                  connection.getOrderBook(new PublicKey(pair.address)).then(book => ({
                    pair: pair.symbol,
                    book,
                  }))
                )
              ),
            ]);

            set(state => ({
              ...state,
              tradePairs: updatedPairs,
              orderBooks: orderBooks.reduce((acc, { pair, book }) => ({
                ...acc,
                [pair]: book,
              }), {}),
              lastUpdated: Date.now(),
              status: 'idle',
            }));
          } catch (error) {
            set({ status: 'error', error: error as Error });
            captureException(error);
          }
        },

        addOrder: async (pair, order) => {
          const { connection, wallet, orderBooks } = get();
          if (!wallet?.publicKey) {
            notify.error('Wallet not connected');
            return;
          }

          try {
            set({ status: 'loading' });
            
            const pairConfig = get().tradePairs.find(p => p.symbol === pair);
            if (!pairConfig) throw new Error('Invalid trading pair');

            const orderBookPubkey = new PublicKey(pairConfig.orderBook);
            const orderId = await submitOrder({
              connection,
              wallet,
              order: {
                ...order,
                price: new BN(order.price * 1e6),
                quantity: new BN(order.quantity * 1e6),
              },
              orderBook: orderBookPubkey,
            });

            set(
              produce<MarketStoreState>(state => {
                const book = state.orderBooks[pair];
                if (order.side === 'buy') {
                  book.bids.push({ ...order, id: orderId });
                } else {
                  book.asks.push({ ...order, id: orderId });
                }
              })
            );
          } catch (error) {
            set({ status: 'error', error: error as Error });
            captureException(error);
            throw error;
          } finally {
            set({ status: 'idle' });
          }
        },

        cancelOrder: async (orderId) => {
          // Implementation similar to addOrder with cancellation logic
        },

        updateLiquidity: (pair, baseDelta, quoteDelta) => {
          set(
            produce<MarketStoreState>(state => {
              const pool = state.liquidityPools.find(p => p.pair === pair);
              if (pool) {
                pool.baseReserve += baseDelta;
                pool.quoteReserve += quoteDelta;
              }
            })
          );
        },

        setBaseCurrency: (currency) => {
          set({ baseCurrency: currency });
        },

        subscribeToPair: (pair) => {
          const { connection } = get();
          const subscriptionId = connection.onOrderBookChange(
            new PublicKey(pair),
            (orderBook) => {
              set(state => ({
                orderBooks: {
                  ...state.orderBooks,
                  [pair]: orderBook,
                },
              }));
            }
          );

          return () => connection.removeListener(subscriptionId);
        },

        reset: () => set(initialState),
      })),
      {
        name: 'market-storage',
        partialize: (state) => 
          Object.fromEntries(
            Object.entries(state).filter(([key]) => 
              !['connection', 'wallet', 'status', 'error'].includes(key)
            )
          ),
      }
    )
  )
);

// Utility types
export interface Order {
  id: string;
  price: number;
  quantity: number;
  side: 'buy' | 'sell';
  timestamp: number;
}

export interface AIModelListing {
  modelId: string;
  owner: string;
  pricePerInference: number;
  throughput: number;
  accuracy: number;
  stakeRequired: number;
}
