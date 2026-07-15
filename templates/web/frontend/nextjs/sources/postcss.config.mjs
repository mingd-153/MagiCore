module.exports = {
  plugins: {
    "postcss-modules": {
      getJSON: (cssFileName, json) => {
        const path = require("path");
        const name = path.basename(cssFileName, ".module.css");
        const outFile = cssFileName.replace(".module.css", ".module.json");
        require("fs").writeFileSync(outFile, JSON.stringify(json));
      },
    },
  },
};