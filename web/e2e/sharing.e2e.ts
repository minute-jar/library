import { expect, test } from '@playwright/test';

test('a parent reads and accepts the sharing terms before the first share', async ({ page }) => {
	await page.goto('/account');
	await page.getByRole('link', { name: 'Sign in with your Minute Jar account' }).click();
	await page.getByRole('textbox', { name: 'Pseudonym' }).fill('Kestrel');
	await page.getByRole('radio', { name: /A separate account for the library/ }).check();
	await page.getByRole('checkbox', { name: /I am an adult and a parent/ }).check();
	await page.getByRole('button', { name: 'Open my library account' }).click();

	await page.getByRole('link', { name: 'Sharing' }).click();
	await expect(page.getByRole('heading', { name: 'The sharing terms' })).toBeVisible();
	await expect(page.getByText(/CC BY-SA 4.0/)).toBeVisible();
	await expect(page.getByText('You may share 10 more today, out of 10 a day.')).toBeVisible();
	await expect(page.getByText('You have shared nothing yet.')).toBeVisible();

	await page.getByRole('checkbox', { name: 'I accept the sharing terms' }).check();
	await page.getByRole('button', { name: 'Accept the sharing terms' }).click();

	await expect(page.getByText(/You accepted the sharing terms/)).toBeVisible();
	await expect(page.getByRole('button', { name: 'Accept the sharing terms' })).toHaveCount(0);
});
