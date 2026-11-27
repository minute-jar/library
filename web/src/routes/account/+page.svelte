<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { getLocale, localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();

	const href = (path: string) => resolve(localizeHref(path) as Path);

	const refusals: Record<string, () => string> = {
		bad_pseudonym: m.refused_bad_pseudonym,
		pseudonym_taken: m.refused_pseudonym_taken,
		statement_required: m.refused_statement_required,
		choice_required: m.refused_choice_required,
		strong_sign_in_required: m.refused_strong_sign_in_required,
		account_exists: m.refused_account_exists,
		confirmation_required: m.refused_confirmation_required
	};
	const code = $derived(form && 'refused' in form ? form.refused : null);
	const refused = $derived(code ? (refusals[code] ?? m.refused_other)() : null);
	const mustSignInAgain = $derived(!data.strong || code === 'strong_sign_in_required');
	const opened = (date: string) =>
		new Intl.DateTimeFormat(getLocale(), { dateStyle: 'long' }).format(new Date(date));
</script>

<svelte:head>
	<title>{m.account_title()} · {m.site_name()}</title>
</svelte:head>

<h1 class="mb-1 text-2xl font-semibold">{m.account_title()}</h1>
<p class="mb-6 text-muted-foreground">{m.account_why()} {m.account_anonymous()}</p>

{#if data.signInFailed}
	<p role="alert" class="mb-4 text-destructive">{m.sign_in_failed()}</p>
{/if}
{#if refused}
	<p role="alert" class="mb-4 text-destructive">{refused}</p>
{/if}
{#if form && 'closed' in form}
	<p role="status" class="mb-4">{m.closed()}</p>
{/if}

{#if !data.signedIn}
	<Button href={href('/sign-in')} data-sveltekit-reload>{m.sign_in()}</Button>
{:else if data.account}
	<Card.Root class="max-w-xl">
		<Card.Header>
			<Card.Description>{m.profile_pseudonym()}</Card.Description>
			<Card.Title><h2 class="text-xl">{data.account.pseudonym}</h2></Card.Title>
		</Card.Header>
		<Card.Content class="grid gap-4">
			<p class="text-sm">
				{data.account.minute_jar_account === 'same_as_subscription'
					? m.profile_same()
					: m.profile_separate()}
				{m.profile_opened({ date: opened(data.account.opened_at) })}
			</p>
			<p class="text-sm text-muted-foreground">{m.choice_others()}</p>
			<p class="text-sm"><a href={href('/account/card')} class="underline">{m.card_link()}</a></p>
			<p class="text-sm">
				<a href={href('/account/sharing')} class="underline">{m.sharing_link()}</a>
			</p>
			<p class="text-sm">
				<a href={href('/account/suggestions')} class="underline">{m.suggestions_link()}</a>
			</p>
			<p class="text-sm">
				<a href={href('/account/items')} class="underline">{m.items_link()}</a>
			</p>
			{#if form && 'renamed' in form}
				<p role="status" class="text-sm">{m.renamed()}</p>
			{/if}
			<form method="post" action="?/pseudonym" class="flex flex-wrap items-end gap-2">
				<label class="grid flex-1 gap-1 text-sm">
					{m.pseudonym_label()}
					<Input
						name="pseudonym"
						required
						minlength={3}
						maxlength={32}
						value={form && 'pseudonym' in form ? form.pseudonym : data.account.pseudonym}
					/>
				</label>
				<Button type="submit" variant="outline">{m.rename_submit()}</Button>
			</form>
		</Card.Content>
	</Card.Root>

	<section class="mt-8 grid max-w-xl gap-3" aria-labelledby="close-title">
		<h2 id="close-title" class="text-lg font-semibold">{m.close_title()}</h2>
		<p class="text-sm">{m.close_what()}</p>
		<p class="text-sm text-muted-foreground">{m.close_kept()}</p>
		{#if mustSignInAgain}
			<p class="text-sm">{m.close_strong()}</p>
			<div>
				<Button href={href('/sign-in?strong')} data-sveltekit-reload variant="outline">
					{m.sign_in_strong_button()}
				</Button>
			</div>
		{:else}
			<form method="post" action="?/close" class="grid gap-3">
				<label class="flex gap-3 text-sm">
					<input type="checkbox" name="understood" required />
					<span>{m.close_understood()}</span>
				</label>
				<div>
					<Button type="submit" variant="destructive">{m.close_submit()}</Button>
				</div>
			</form>
		{/if}
	</section>
{:else if mustSignInAgain}
	<Card.Root class="max-w-xl">
		<Card.Header>
			<Card.Title><h2>{m.sign_in_strong_title()}</h2></Card.Title>
		</Card.Header>
		<Card.Content class="grid gap-4">
			<p>{m.sign_in_strong()}</p>
			<div>
				<Button href={href('/sign-in?strong')} data-sveltekit-reload>
					{m.sign_in_strong_button()}
				</Button>
			</div>
		</Card.Content>
	</Card.Root>
{:else}
	<form method="post" action="?/open" class="grid max-w-xl gap-6">
		<h2 class="text-xl font-semibold">{m.open_title()}</h2>
		<label class="grid gap-1 text-sm">
			{m.pseudonym_label()}
			<Input
				name="pseudonym"
				required
				minlength={3}
				maxlength={32}
				value={form && 'pseudonym' in form ? form.pseudonym : ''}
				aria-describedby="pseudonym-hint"
			/>
			<span id="pseudonym-hint" class="text-muted-foreground">{m.pseudonym_hint()}</span>
		</label>

		<fieldset class="grid gap-3">
			<legend class="mb-2 font-medium">{m.choice_legend()}</legend>
			<label class="flex gap-3 rounded-lg border p-3 text-sm">
				<input
					type="radio"
					name="minute_jar_account"
					value="same_as_subscription"
					required
					checked={form && 'choice' in form && form.choice === 'same_as_subscription'}
				/>
				<span class="grid gap-1">
					<span class="font-medium">{m.choice_same()}</span>
					<span>{m.choice_same_operator()}</span>
					<span class="text-muted-foreground">{m.choice_others()}</span>
				</span>
			</label>
			<label class="flex gap-3 rounded-lg border p-3 text-sm">
				<input
					type="radio"
					name="minute_jar_account"
					value="separate"
					checked={form && 'choice' in form && form.choice === 'separate'}
				/>
				<span class="grid gap-1">
					<span class="font-medium">{m.choice_separate()}</span>
					<span>{m.choice_separate_operator()}</span>
					<span class="text-muted-foreground">{m.choice_others()}</span>
				</span>
			</label>
		</fieldset>

		<label class="flex gap-3 text-sm">
			<input type="checkbox" name="adult_and_parent" required />
			<span>{m.statement_label()}</span>
		</label>

		<div>
			<Button type="submit">{m.open_submit()}</Button>
		</div>
	</form>
{/if}

{#if data.signedIn}
	<form method="post" action={href('/sign-out')} class="mt-8">
		<Button type="submit" variant="link" class="px-0">{m.sign_out()}</Button>
	</form>
{/if}
