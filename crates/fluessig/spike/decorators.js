// No-op impls: TypeSpec records decorator applications + args on the checked
// program regardless of what the implementation does.
function noop() {}

export const $decorators = {
  Fluessig: {
    entity: noop,
    key: noop,
    edge: noop,
    compose: noop,
    ctor: noop,
    stream: noop,
    manual: noop,
  },
};
