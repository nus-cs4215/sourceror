var test = require('ava');
var browserEnv = require('browser-env');
browserEnv();

test.skip('Insert to DOM', t => {
	const div = document.createElement('div');
	document.body.appendChild(div);
	t.is(document.querySelector('div'), div);
});

test.todo('Maybe we can do something with the mocked browser environment');
