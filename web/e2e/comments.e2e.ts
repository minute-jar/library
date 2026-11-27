import { expect, test } from '@playwright/test';

test('a parent with a library account comments on an item under their pseudonym', async ({
	page
}) => {
	await page.goto('/items/0123456789abcdef0123456789abcdef');
	await expect(
		page.getByRole('link', { name: 'To comment, sign in with your library account.' })
	).toBeVisible();

	await page.goto('/account');
	await page.getByRole('link', { name: 'Sign in with your Minute Jar account' }).click();
	await page.getByRole('textbox', { name: 'Pseudonym' }).fill('Heron');
	await page.getByRole('radio', { name: /A separate account for the library/ }).check();
	await page.getByRole('checkbox', { name: /I am an adult and a parent/ }).check();
	await page.getByRole('button', { name: 'Open my library account' }).click();

	await page.goto('/items/0123456789abcdef0123456789abcdef');
	await page.getByRole('textbox', { name: 'Your comment' }).fill('We do five verbs a day.');
	await page.getByRole('button', { name: 'Comment', exact: true }).click();

	await expect(page.getByText('Your comment is published.')).toBeVisible();
	const comment = page.getByRole('listitem').filter({ hasText: 'We do five verbs a day.' });
	await expect(comment).toContainText('Heron');

	await comment.getByText('Report this comment').click();
	await comment.getByRole('combobox', { name: 'Reason' }).selectOption('unsuitable_content');
	await comment.getByRole('button', { name: 'Send the report' }).click();
	await expect(page.getByText('Thank you: a moderator will look at this comment.')).toBeVisible();
});
