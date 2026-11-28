import { expect, test } from '@playwright/test';

test('a parent signs in and opens a library account under a pseudonym', async ({ page }) => {
	await page.goto('/');
	await page.getByRole('link', { name: 'Library account' }).click();
	await expect(page.getByText('Your real name never appears.')).toBeVisible();

	await page.getByRole('link', { name: 'Sign in with your Minute Jar account' }).click();
	await expect(page).toHaveURL(/\/account$/);
	await expect(page.getByRole('heading', { name: 'Open your library account' })).toBeVisible();

	await page.getByRole('textbox', { name: 'Pseudonym' }).fill('Owl');
	await expect(page.getByText(/What the operator knows/).first()).toBeVisible();
	await expect(page.getByText(/What others see: your pseudonym only/).first()).toBeVisible();
	await page.getByRole('radio', { name: /A separate account for the library/ }).check();
	await page.getByRole('checkbox', { name: /I am an adult and a parent/ }).check();
	await page.getByRole('button', { name: 'Open my library account' }).click();

	await expect(page.getByRole('heading', { name: 'Owl' })).toBeVisible();
	await expect(
		page.getByText('On a Minute Jar account separate from any subscription.')
	).toBeVisible();

	await page.getByRole('button', { name: 'Sign out' }).click();
	await page.getByRole('link', { name: 'Library account' }).click();
	await expect(
		page.getByRole('link', { name: 'Sign in with your Minute Jar account' })
	).toBeVisible();
});

test('a parent closes their library account and is signed out', async ({ page }) => {
	await page.goto('/account');
	await page.getByRole('link', { name: 'Sign in with your Minute Jar account' }).click();
	await page.getByRole('textbox', { name: 'Pseudonym' }).fill('Wren');
	await page.getByRole('radio', { name: /A separate account for the library/ }).check();
	await page.getByRole('checkbox', { name: /I am an adult and a parent/ }).check();
	await page.getByRole('button', { name: 'Open my library account' }).click();
	await expect(page.getByRole('heading', { name: 'Wren' })).toBeVisible();

	await expect(
		page.getByText(/Your shared items stay published under their licence/)
	).toBeVisible();
	await page.getByRole('checkbox', { name: /my shared items stay published/ }).check();
	await page.getByRole('button', { name: 'Close my library account' }).click();

	await expect(page.getByRole('status')).toHaveText(/Your library account is closed/);
	await expect(
		page.getByRole('link', { name: 'Sign in with your Minute Jar account' })
	).toBeVisible();
});

test('an author chooses to show the items of earlier pseudonyms on their profile', async ({
	page
}) => {
	await page.goto('/account');
	await page.getByRole('link', { name: 'Sign in with your Minute Jar account' }).click();
	await page.getByRole('textbox', { name: 'Pseudonym' }).fill('Merlin');
	await page.getByRole('radio', { name: /A separate account for the library/ }).check();
	await page.getByRole('checkbox', { name: /I am an adult and a parent/ }).check();
	await page.getByRole('button', { name: 'Open my library account' }).click();
	await expect(page.getByRole('heading', { name: 'Merlin' })).toBeVisible();

	const earlier = page.getByRole('checkbox', {
		name: /items published under my earlier pseudonyms/
	});
	await expect(earlier).not.toBeChecked();
	await earlier.check();
	await page.getByRole('button', { name: 'Save this choice' }).click();

	await expect(page.getByRole('status')).toHaveText('Your choice is saved.');
	await page.reload();
	await expect(
		page.getByRole('checkbox', { name: /items published under my earlier pseudonyms/ })
	).toBeChecked();
});
