import tseslint from 'typescript-eslint';

export default tseslint.config(
  { files: ['scripts/crash-*.mts', 'scripts/native-adapter-*.mts'], extends: [tseslint.configs.recommended],
    rules: { '@typescript-eslint/no-non-null-assertion': 'off' } },
);
