<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { getLocale, localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();

	const href = (path: string) => resolve(localizeHref(path) as Path);

	const outcomes: Record<string, () => string> = {
		card_check_pending: m.card_pending,
		card_refused: m.card_refused,
		card_verified: m.refused_card_verified,
		strong_sign_in_required: m.refused_strong_sign_in_required
	};
	const code = $derived(form && 'refused' in form ? form.refused : data.outcome);
	const said = $derived(code ? (outcomes[code] ?? m.refused_other)() : null);
	const mustSignInAgain = $derived(!data.strong || code === 'strong_sign_in_required');
	const day = (date: string) =>
		new Intl.DateTimeFormat(getLocale(), { dateStyle: 'long' }).format(new Date(date));
</script>

<svelte:head>
	<title>{m.card_title()} · {m.site_name()}</title>
</svelte:head>

<h1 class="mb-1 text-2xl font-semibold">{m.card_title()}</h1>
<p class="mb-6 max-w-xl text-muted-foreground">{m.card_why()}</p>

{#if said && !data.card}
	<p role="alert" class="mb-4 text-destructive">{said}</p>
{/if}

{#if data.card}
	<p role="status" class="max-w-xl">
		{data.card.verified_by === 'subscription'
			? m.card_verified_subscription({ date: day(data.card.verified_at) })
			: m.card_verified_check({ date: day(data.card.verified_at) })}
	</p>
{:else if mustSignInAgain}
	<Card.Root class="max-w-xl">
		<Card.Header>
			<Card.Title><h2>{m.sign_in_strong_title()}</h2></Card.Title>
		</Card.Header>
		<Card.Content class="grid gap-4">
			<p>{m.card_sign_in_strong()}</p>
			<div>
				<Button href={href('/sign-in?strong')} data-sveltekit-reload>
					{m.sign_in_strong_button()}
				</Button>
			</div>
		</Card.Content>
	</Card.Root>
{:else}
	<form method="post" action="?/verify" class="grid max-w-xl gap-4">
		{#if data.subscription}
			<p>{m.card_subscription()}</p>
		{/if}
		<p>{m.card_check()}</p>
		<div>
			<Button type="submit">{m.card_submit()}</Button>
		</div>
	</form>
{/if}

<p class="mt-8">
	<a href={href('/account')} class="text-sm underline">{m.account_title()}</a>
</p>
