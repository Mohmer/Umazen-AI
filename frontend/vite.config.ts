import { defineConfig, splitVendorChunkPlugin } from 'vite'
import react from '@vitejs/plugin-react'
import { visualizer } from 'rollup-plugin-visualizer'
import svgr from 'vite-plugin-svgr'
import { nodePolyfills } from 'vite-plugin-node-polyfills'
import checker from 'vite-plugin-checker'
import { VitePWA } from 'vite-plugin-pwa'
import { comlink } from 'vite-plugin-comlink'
import tsconfigPaths from 'vite-tsconfig-paths'

// https://vitejs.dev/config/
export default defineConfig(({ mode }) => ({
  base: './',
  publicDir: 'public',
  assetsInclude: ['**/*.wasm'],
  
  plugins: [
    // Core framework integration
    react({
      babel: {
        plugins: [
          ['@babel/plugin-proposal-decorators', { legacy: true }],
          'babel-plugin-styled-components'
        ]
      }
    }),
    
    // SVG component support
    svgr({
      svgrOptions: {
        icon: true,
        svgoConfig: {
          plugins: [
            { name: 'removeViewBox', active: false },
            { name: 'convertColors', params: { currentColor: true } }
          ]
        }
      }
    }),

    // Node.js polyfills for blockchain interactions
    nodePolyfills({
      protocolImports: true,
      include: ['buffer', 'stream', 'util', 'crypto']
    }),

    // TypeScript validation
    checker({
      typescript: true,
      eslint: { lintCommand: 'eslint "./src/**/*.{ts,tsx}"' },
      overlay: { initialIsOpen: false }
    }),

    // Progressive Web App support
    VitePWA({
      registerType: 'autoUpdate',
      workbox: {
        globPatterns: ['**/*.{js,css,html,ico,png,svg,wasm}'],
        maximumFileSizeToCacheInBytes: 10 * 1024 * 1024
      },
      manifest: {
        name: 'Umazen AI Platform',
        short_name: 'Umazen',
        theme_color: '#1a1a1a',
        background_color: '#0f172a',
        display: 'standalone'
      }
    }),

    // Web Worker optimization
    comlink(),

    // Path aliases from tsconfig
    tsconfigPaths(),

    // Bundle analysis (only in report mode)
    mode === 'report' && visualizer({
      filename: 'dist/stats.html',
      gzipSize: true,
      brotliSize: true
    }),

    // Vendor chunk splitting
    splitVendorChunkPlugin()
  ].filter(Boolean),

  resolve: {
    alias: {
      // Solana Web3.js compatibility
      'readable-stream': 'vite-compatible-readable-stream'
    }
  },

  build: {
    target: 'es2022',
    sourcemap: mode !== 'production',
    minify: mode === 'production' ? 'terser' : false,
    reportCompressedSize: false,
    
    rollupOptions: {
      output: {
        manualChunks: {
          'solana': ['@solana/web3.js', '@solana/spl-token'],
          'anchor': ['@coral-xyz/anchor'],
          'react-vendor': ['react', 'react-dom', 'react-router-dom'],
          'ai': ['@tensorflow/tfjs', '@huggingface/transformers']
        },
        chunkFileNames: 'assets/[name]-[hash].js',
        entryFileNames: 'assets/[name]-[hash].js',
        assetFileNames: 'assets/[name]-[hash][extname]'
      }
    },

    terserOptions: {
      compress: {
        drop_console: mode === 'production',
        ecma: 2020,
        passes: 3
      },
      format: {
        comments: false,
        ecma: 2020
      }
    }
  },

  optimizeDeps: {
    esbuildOptions: {
      target: 'es2022',
      define: {
        global: 'globalThis'
      }
    },
    include: [
      '@solana/web3.js',
      '@solana/spl-token',
      '@coral-xyz/anchor',
      'buffer',
      'process'
    ]
  },

  server: {
    port: 3000,
    strictPort: true,
    open: true,
    proxy: {
      '/api': {
        target: 'http://localhost:3001',
        changeOrigin: true,
        rewrite: path => path.replace(/^\/api/, ''),
        ws: true
      }
    }
  },

  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html']
    }
  },

  worker: {
    format: 'es',
    plugins: [comlink()]
  }
}))
