<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { getLocale, locales, localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import './layout.css';
	import favicon from '#lib/assets/favicon.svg';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	/** Each language, named in itself. */
	const names: Record<string, string> = { en: 'English', fr: 'Français' };
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>{m.site_name()}</title>
	<meta name="description" content={m.site_tagline()} />
</svelte:head>

<div class="mx-auto flex min-h-screen max-w-5xl flex-col px-4">
	<header class="flex flex-wrap items-center justify-between gap-4 border-b py-4">
		<a href={resolve(localizeHref('/') as Path)} class="text-lg font-semibold">{m.site_name()}</a>
		<a
			href={resolve(localizeHref('/account') as Path)}
			class="ml-auto text-sm underline-offset-4 hover:underline"
		>
			{m.account_link()}
		</a>
		<nav aria-label={m.language_switch()} class="flex gap-3 text-sm">
			{#each locales as locale (locale)}
				<a
					href={resolve(localizeHref(page.url.pathname + page.url.search, { locale }) as Path)}
					hreflang={locale}
					lang={locale}
					aria-current={locale === getLocale() ? 'true' : undefined}
					class="underline-offset-4 hover:underline aria-[current]:font-semibold"
					data-sveltekit-reload
				>
					{names[locale] ?? locale}
				</a>
			{/each}
		</nav>
	</header>

	<main class="flex-1 py-6">
		{@render children()}
	</main>

	<footer class="border-t py-4 text-sm text-muted-foreground">
		{m.footer_no_trackers()}
	</footer>
</div>
