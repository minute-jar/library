import { readFile } from 'node:fs/promises';
import { expect, test } from '@playwright/test';
import { verbsFile } from '../src/lib/server/testing/fake-api.ts';

test('a visitor opens an item from the catalogue and downloads it as a file', async ({ page }) => {
	await page.goto('/');
	await page.getByRole('link', { name: 'Irregular verbs, part 1' }).click();

	await expect(page).toHaveURL(/\/items\/0123456789abcdef0123456789abcdef$/);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Irregular verbs, part 1');
	await expect(page.getByText("Past of 'go'")).toBeVisible();
	await expect(page.getByText('Strong verbs')).toBeVisible();
	await expect(page.getByText('Learnt in a week.')).toBeVisible();
	await expect(page.getByRole('link', { name: 'Download version 1 as a file' })).toBeVisible();

	const downloading = page.waitForEvent('download');
	await page.getByRole('link', { name: 'Download version 2 as a file' }).click();
	const download = await downloading;

	expect(download.suggestedFilename()).toBe('minute-jar-0123456789abcdef0123456789abcdef-2.json');
	expect(await readFile(await download.path(), 'utf8')).toBe(verbsFile);
});

test('a derived work credits its original and links to it', async ({ page }) => {
	await page.goto('/items/00112233445566778899aabbccddeeff');

	await expect(page.getByText('Derived from a work by Owl')).toBeVisible();
	await page
		.getByRole('link', { name: 'See the original: Irregular verbs, part 1, version 1' })
		.click();

	await expect(page).toHaveURL(/\/items\/0123456789abcdef0123456789abcdef$/);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Irregular verbs, part 1');
});
