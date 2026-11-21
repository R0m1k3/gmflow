import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [
    sveltekit({
      // SPA : toute route inconnue renvoie index.html, servi par l'app Rust.
      adapter: adapter({ fallback: 'index.html' })
    })
  ],
  server: {
    // En développement : l'API tourne sur 8347 (GMFLOW_PORT).
    proxy: { '/api': 'http://localhost:8347', '/health': 'http://localhost:8347' }
  }
});
