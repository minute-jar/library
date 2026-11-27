import { expect, test } from '@playwright/test';

test('an author withdraws one version of their item, for good', async ({ page }) => {
	await page.goto('/account');
	await page.getByRole('link', { name: 'Sign in with your Minute Jar account' }).click();
	await page.getByRole('textbox', { name: 'Pseudonym' }).fill('Harrier');
	await page.getByRole('radio', { name: /A separate account for the library/ }).check();
	await page.getByRole('checkbox', { name: /I am an adult and a parent/ }).check();
	await page.getByRole('button', { name: 'Open my library account' }).click();

	await page.getByRole('link', { name: 'Your items' }).click();
	await expect(page.getByRole('heading', { name: 'Irregular verbs, part 2' })).toBeVisible();
	await expect(page.getByText(/It cannot be undone/)).toBeVisible();

	await page.getByRole('combobox', { name: 'What to withdraw' }).selectOption('Version 2 only');
	await page.getByRole('button', { name: 'Withdraw' }).click();
	await expect(page.getByRole('alert')).toHaveCount(0);
	await page.getByRole('checkbox', { name: /cannot be undone/ }).check();
	await page.getByRole('button', { name: 'Withdraw' }).click();

	await expect(page.getByRole('status')).toHaveText('Withdrawn.');
	await expect(page.getByText(/Withdrawn by you on/)).toBeVisible();
	await expect(page.getByRole('option', { name: 'Version 2 only' })).toHaveCount(0);
});
