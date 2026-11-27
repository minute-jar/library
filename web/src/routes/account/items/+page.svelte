<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { getLocale, localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import type { Withdrawal } from '#lib/server/authored.ts';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();

	const href = (path: string) => resolve(localizeHref(path) as Path);

	const refusals: Record<string, () => string> = {
		confirmation_required: m.refused_withdrawal_confirmation,
		strong_sign_in_required: m.refused_strong_sign_in_required,
		already_withdrawn: m.refused_already_withdrawn
	};
	const code = $derived(form && 'refused' in form ? form.refused : null);
	const refused = $derived(code ? (refusals[code] ?? m.refused_other)() : null);
	const refusedItem = $derived(form && 'item' in form ? form.item : null);
	const withdrawnItem = $derived(form && 'withdrawn' in form ? form.withdrawn : null);
	const mustSignInAgain = $derived(!data.strong || code === 'strong_sign_in_required');
	const day = (date: string) =>
		new Intl.DateTimeFormat(getLocale(), { dateStyle: 'long', timeZone: 'UTC' }).format(
			new Date(date)
		);
	const withdrawn = (withdrawal: Withdrawal) => {
		const date = day(withdrawal.withdrawn_at);
		if (withdrawal.kind === 'author') return m.items_withdrawn_author({ date });
		if (withdrawal.kind === 'moderation') return m.items_withdrawn_moderation({ date });
		return m.items_withdrawn_switch_off({ date });
	};
</script>

<svelte:head>
	<title>{m.items_title()} · {m.site_name()}</title>
</svelte:head>

<h1 class="mb-1 text-2xl font-semibold">{m.items_title()}</h1>
<p class="mb-2 max-w-xl text-muted-foreground">{m.items_why()}</p>
<p class="mb-6 max-w-xl text-sm">{m.items_withdraw_what()}</p>

{#if refused && !refusedItem}
	<p role="alert" class="mb-4 text-destructive">{refused}</p>
{/if}

{#if mustSignInAgain && data.items.length > 0}
	<div class="mb-6 grid max-w-xl gap-2">
		<p class="text-sm">{m.items_strong()}</p>
		<div>
			<Button href={href('/sign-in?strong')} data-sveltekit-reload variant="outline">
				{m.sign_in_strong_button()}
			</Button>
		</div>
	</div>
{/if}

{#if data.items.length === 0}
	<p>{m.items_none()}</p>
{:else}
	<ul class="grid max-w-xl gap-4">
		{#each data.items as item (item.item)}
			{@const open = item.versions.filter((version) => !version.withdrawal)}
			<li class="grid gap-2 rounded-lg border p-4">
				<h2 class="font-medium">
					{#if !item.withdrawal && open.length > 0}
						<a href={href(`/items/${item.item}`)} class="underline">{item.title}</a>
					{:else}
						{item.title}
					{/if}
				</h2>
				{#if item.withdrawal}
					<p>{withdrawn(item.withdrawal)}</p>
				{/if}
				<ul class="grid gap-1 text-sm">
					{#each item.versions as version (version.version)}
						<li>
							{m.items_version({ version: version.version, date: day(version.published_at) })}
							{#if version.withdrawal}
								<span class="text-muted-foreground">{withdrawn(version.withdrawal)}</span>
							{/if}
						</li>
					{/each}
				</ul>
				{#if withdrawnItem === item.item}
					<p role="status">{m.items_withdrawn_now()}</p>
				{/if}
				{#if refusedItem === item.item && refused}
					<p role="alert" class="text-destructive">{refused}</p>
				{/if}
				{#if item.withdrawal?.kind !== 'author' && !mustSignInAgain}
					<form method="post" action="?/withdraw" class="grid gap-2">
						<input type="hidden" name="item" value={item.item} />
						<label class="grid gap-1 text-sm">
							{m.items_withdraw_label()}
							<select name="version" class="rounded-md border bg-transparent px-3 py-2">
								<option value="">{m.items_withdraw_whole()}</option>
								{#each item.versions.filter((version) => version.withdrawal?.kind !== 'author') as version (version.version)}
									<option value={String(version.version)}>
										{m.items_withdraw_version({ version: version.version })}
									</option>
								{/each}
							</select>
						</label>
						<label class="flex gap-3 text-sm">
							<input type="checkbox" name="understood" required />
							<span>{m.items_understood()}</span>
						</label>
						<div>
							<Button type="submit" variant="destructive">{m.items_withdraw_submit()}</Button>
						</div>
					</form>
				{/if}
			</li>
		{/each}
	</ul>
{/if}

<p class="mt-8">
	<a href={href('/account')} class="text-sm underline">{m.account_title()}</a>
</p>
