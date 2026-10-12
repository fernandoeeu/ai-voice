import { describe, expect, test } from 'bun:test';
// @ts-expect-error módulo .mjs sem tipos
import { hostLocal } from '../scripts/hosts-locais.mjs';

describe('hostLocal', () => {
	test.each(['127.0.0.1:7350', 'localhost:7350', '[::1]:7350', '192.168.0.12:7350', '10.1.2.3', '172.16.0.5:7350', 'mac-do-fernando.local:7350', 'macbook'])(
		'aceita %s',
		(h) => expect(hostLocal(h)).toBe(true)
	);
	test.each(['evil.com', 'evil.com:7350', '8.8.8.8:7350', '172.32.0.1', '192.169.0.1', '999.1.1.1', 'a.b.example', ''])('recusa %s', (h) =>
		expect(hostLocal(h)).toBe(false)
	);
});
