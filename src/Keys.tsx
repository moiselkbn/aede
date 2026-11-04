/** The key caps of a shortcut label such as "Ctrl+Option+Espace". */
function Keys({ label }: { label: string }) {
  return (
    <span className="keys">
      {label.split("+").map((key, index) => (
        <kbd className="key" key={index}>
          {key}
        </kbd>
      ))}
    </span>
  );
}

export default Keys;
