import { expect, test } from '@playwright/test';

test('a parent without a subscription verifies a card by a zero-amount check', async ({ page }) => {
	await page.goto('/account');
	await page.getByRole('link', { name: 'Sign in with your Minute Jar account' }).click();
	await page.getByRole('textbox', { name: 'Pseudonym' }).fill('Heron');
	await page.getByRole('radio', { name: /A separate account for the library/ }).check();
	await page.getByRole('checkbox', { name: /I am an adult and a parent/ }).check();
	await page.getByRole('button', { name: 'Open my library account' }).click();

	await page.getByRole('link', { name: 'Your card' }).click();
	await expect(page.getByRole('heading', { name: 'Your card' })).toBeVisible();
	await expect(page.getByText(/never sees the card number/)).toBeVisible();
	await expect(page.getByText(/Nothing is charged/)).toBeVisible();
	await page.getByRole('button', { name: 'Verify my card' }).click();

	await expect(page).toHaveURL(/\/account\/card$/);
	await expect(page.getByText(/Your card is verified, by a zero-amount check/)).toBeVisible();
	await expect(page.getByRole('button', { name: 'Verify my card' })).toHaveCount(0);
});
