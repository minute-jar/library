import { expect, test } from '@playwright/test';

test('an author accepts answers families suggested into a new version', async ({ page }) => {
	await page.goto('/account');
	await page.getByRole('link', { name: 'Sign in with your Minute Jar account' }).click();
	await page.getByRole('textbox', { name: 'Pseudonym' }).fill('Plover');
	await page.getByRole('radio', { name: /A separate account for the library/ }).check();
	await page.getByRole('checkbox', { name: /I am an adult and a parent/ }).check();
	await page.getByRole('button', { name: 'Open my library account' }).click();

	await page.getByRole('link', { name: 'Suggestions from families' }).click();
	await expect(page.getByRole('heading', { name: 'Irregular verbs' })).toBeVisible();
	await expect(page.getByText("Question 1 · Past of 'go'")).toBeVisible();
	await expect(page.getByText('Suggested by 2 families')).toBeVisible();

	await page.getByRole('button', { name: 'Share a new version with these answers' }).click();
	await expect(page.getByText('Please tick at least one answer to accept.')).toBeVisible();

	await page.getByRole('checkbox', { name: /goed/ }).check();
	await page.getByRole('button', { name: 'Share a new version with these answers' }).click();
	await expect(
		page.getByText('Version 2 is published with the answers you accepted.')
	).toBeVisible();
});
