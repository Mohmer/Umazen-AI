// src/main.tsx

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter as Router } from 'react-router-dom';
import { Provider as ReduxProvider } from 'react-redux';
import { ThemeProvider } from '@mui/material/styles';
import { AdapterDayjs } from '@mui/x-date-pickers/AdapterDayjs';
import { LocalizationProvider } from '@mui/x-date-pickers';
import { WalletProvider } from '@solana/wallet-adapter-react';
import { WalletModalProvider } from '@solana/wallet-adapter-react-ui';
import { ConnectionProvider } from '@solana/wallet-adapter-react';
import { SnackbarProvider } from 'notistack';
import { HelmetProvider } from 'react-helmet-async';
import { loadErrorMessages, loadDevMessages } from "@apollo/client/dev";
import { registerSW } from 'virtual:pwa-register';
import { Workbox } from 'workbox-window';

// Core Configuration
import { store } from './app/store';
import { theme } from './app/theme';
import { SOLANA_RPC_ENDPOINT } from './config';
import { initAnalytics } from './lib/analytics';
import { initSentry } from './lib/sentry';
import { apolloClient } from './graphql/client';
import { ApiClientProvider } from './api';
import { DynamicFeatureLoader } from './features';

// Global Styles
import './app/i18n';
import '@solana/wallet-adapter-react-ui/styles.css';
import './app/styles/global.scss';

// AI Model Loader
import { initAIModels } from './lib/ai/loader';

// Error Handling
import { GlobalErrorBoundary } from './app/error-handling';

// Lazy Components
const AppLazy = React.lazy(() => import('./app/App'));

// Initialize monitoring
if (import.meta.env.PROD) {
  initSentry();
  initAnalytics();
}

// Load Apollo Client Dev Messages
if (import.meta.env.DEV) {
  loadDevMessages();
  loadErrorMessages();
}

// Service Worker Registration
const updateSW = registerSW({
  onNeedRefresh() {
    store.dispatch(showServiceWorkerUpdate());
  },
  onOfflineReady() {
    store.dispatch(serviceWorkerReady());
  },
});

// Initialize Workbox
if ('serviceWorker' in navigator) {
  const wb = new Workbox('/sw.js');
  wb.register();
}

// Solana Wallet Configuration
const wallets = initializeWalletAdapters([
  new PhantomWalletAdapter(),
  new SolflareWalletAdapter(),
  new LedgerWalletAdapter(),
  new TorusWalletAdapter(),
]);

// Global Error Handler
window.onunhandledrejection = (event: PromiseRejectionEvent) => {
  captureException(event.reason);
  event.preventDefault();
};

// Main Render Function
async function bootstrap() {
  try {
    // Preload critical assets
    await Promise.all([
      initAIModels(),
      loadFonts(),
      preloadWalletIcons(),
    ]);

    const container = document.getElementById('root')!;
    const root = createRoot(container);

    root.render(
      <StrictMode>
        <GlobalErrorBoundary>
          <HelmetProvider>
            <ReduxProvider store={store}>
              <LocalizationProvider dateAdapter={AdapterDayjs}>
                <ApolloProvider client={apolloClient}>
                  <ConnectionProvider endpoint={SOLANA_RPC_ENDPOINT}>
                    <WalletProvider wallets={wallets} autoConnect>
                      <WalletModalProvider>
                        <SnackbarProvider maxSnack={3}>
                          <ThemeProvider theme={theme}>
                            <Router>
                              <ApiClientProvider>
                                <DynamicFeatureLoader>
                                  <React.Suspense fallback={<FullPageLoader />}>
                                    <AppLazy />
                                  </React.Suspense>
                                </DynamicFeatureLoader>
                              </ApiClientProvider>
                            </Router>
                          </ThemeProvider>
                        </SnackbarProvider>
                      </WalletModalProvider>
                    </WalletProvider>
                  </ConnectionProvider>
                </ApolloProvider>
              </LocalizationProvider>
            </ReduxProvider>
          </HelmetProvider>
        </GlobalErrorBoundary>
      </StrictMode>
    );

    // Initialize post-render logic
    store.dispatch(initWalletConnection());
    store.dispatch(fetchGlobalConfig());
    
    if (import.meta.env.DEV) {
      initViteHMR();
    }
  } catch (error) {
    const fallbackContainer = document.getElementById('root');
    if (fallbackContainer) {
      fallbackContainer.innerHTML = `<h1>Critical initialization failed</h1>`;
    }
    throw error;
  }
}

// Start the application
bootstrap()
  .catch((error) => {
    captureException(error);
    console.error('Bootstrap failed:', error);
  });

// Web Vitals Reporting
reportWebVitals((metric) => {
  if (import.meta.env.PROD) {
    sendToAnalytics(metric);
  }
});

// Hot Module Replacement
if (import.meta.hot) {
  import.meta.hot.accept();
}
