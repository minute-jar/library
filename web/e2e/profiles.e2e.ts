import { expect, test } from '@playwright/test';

test('an author sees their profile with the suggestions received; others see it without', async ({
	page,
	browser
}) => {
	await page.goto('/account');
	await page.getByRole('link', { name: 'Sign in with your Minute Jar account' }).click();
	await page.getByRole('textbox', { name: 'Pseudonym' }).fill('Puffin');
	await page.getByRole('radio', { name: /A separate account for the library/ }).check();
	await page.getByRole('checkbox', { name: /I am an adult and a parent/ }).check();
	await page.getByRole('button', { name: 'Open my library account' }).click();

	await page.getByRole('link', { name: 'Your author profile' }).click();
	await expect(page).toHaveURL(/\/authors\/Puffin$/);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Puffin');
	await expect(page.getByText('1 shared items, used by 3 families.')).toBeVisible();
	await expect(page.getByRole('link', { name: 'Irregular verbs, part 1' })).toBeVisible();
	await expect(page.getByText('Suggestions from 2 families, 3 answers waiting')).toBeVisible();

	const visitor = await (await browser.newContext()).newPage();
	await visitor.goto('/authors/puffin');
	await expect(visitor.getByRole('heading', { level: 1 })).toHaveText('Puffin');
	await expect(visitor.getByText('Imported by 3 families')).toBeVisible();
	await expect(visitor.getByText(/Suggestions from/)).toHaveCount(0);
});

test('an item page links to its author’s profile', async ({ page }) => {
	await page.goto('/items/0123456789abcdef0123456789abcdef');

	await expect(page.getByRole('link', { name: 'by Owl' })).toHaveAttribute('href', '/authors/Owl');
});
