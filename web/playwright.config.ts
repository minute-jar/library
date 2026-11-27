import { defineConfig } from '@playwright/test';

// The website, built, over a stand-in for the catalogue API (`e2e/fake-api.ts`).
export default defineConfig({
	testDir: 'e2e',
	testMatch: '**/*.e2e.ts',
	use: { baseURL: 'http://127.0.0.1:4173' },
	webServer: [
		{ command: 'node e2e/fake-api.ts', url: 'http://127.0.0.1:4174/api/catalogue/filters' },
		{
			command: 'vite build && vite preview --host 127.0.0.1 --port 4173 --strictPort',
			url: 'http://127.0.0.1:4173',
			env: { LIBRARY_API: 'http://127.0.0.1:4174' },
			timeout: 180_000
		}
	]
});
