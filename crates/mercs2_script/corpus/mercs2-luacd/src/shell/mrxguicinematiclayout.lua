LocalWidgetList = {}
if import then
  import("MrxGuiCinematic")
else
  MrxGuiCinematic = {}
end
LocalWidgetList[1] = {
  name = "Cinematic Placeholder",
  x1 = 0,
  y1 = 0,
  x2 = 640,
  y2 = 480,
  RedLevel = 255,
  GreenLevel = 255,
  BlueLevel = 255,
  TranslucencyLevel = 255,
  HorizontalAnchor = "center",
  VerticalAnchor = "center",
  visible = -1,
  container = false,
  EventHandlerFile = "MrxGuiCinematic",
  EventHandlers = {
    GuiInitialization = MrxGuiCinematic._HandleInitializationEvent
  },
  EventHandlerNames = {
    GuiInitialization = "_HandleInitializationEvent"
  },
  Children = {
    {
      name = "placeholder black bg",
      x1 = 0,
      y1 = 0,
      x2 = 640,
      y2 = 480,
      RedLevel = 0,
      GreenLevel = 0,
      BlueLevel = 0,
      TranslucencyLevel = 255,
      HorizontalAnchor = "center",
      VerticalAnchor = "center",
      visible = 1,
      container = false,
      WidgetType = "image",
      texture = nil,
      textureFile = nil,
      rotation = 0,
      u1 = 0,
      v1 = 0,
      u2 = 1,
      v2 = 1,
      EventHandlerFile = "",
      EventHandlers = {},
      EventHandlerNames = {},
      Children = {}
    },
    {
      name = "placeholder image",
      x1 = 0,
      y1 = 60,
      x2 = 640,
      y2 = 420,
      RedLevel = 255,
      GreenLevel = 255,
      BlueLevel = 255,
      TranslucencyLevel = 255,
      HorizontalAnchor = "center",
      VerticalAnchor = "center",
      visible = -1,
      container = false,
      WidgetType = "image",
      texture = nil,
      textureFile = nil,
      rotation = 0,
      u1 = 0,
      v1 = 0,
      u2 = 1,
      v2 = 1,
      EventHandlerFile = "",
      EventHandlers = {},
      EventHandlerNames = {},
      Children = {}
    },
    {
      name = "placeholder text",
      x1 = 70,
      y1 = 390,
      x2 = 570,
      y2 = 406,
      RedLevel = 255,
      GreenLevel = 255,
      BlueLevel = 255,
      TranslucencyLevel = 255,
      HorizontalAnchor = "center",
      VerticalAnchor = "bottom",
      visible = 1,
      container = false,
      WidgetType = "text",
      text = "Cinematic placeholder text.",
      font = "english_18",
      scale = 1,
      Justification = "center",
      EventHandlerFile = "",
      EventHandlers = {},
      EventHandlerNames = {},
      Children = {}
    },
    {
      name = "continue",
      x1 = 70,
      y1 = 420,
      x2 = 244,
      y2 = 436,
      RedLevel = 255,
      GreenLevel = 255,
      BlueLevel = 255,
      TranslucencyLevel = 255,
      HorizontalAnchor = "left",
      VerticalAnchor = "bottom",
      visible = 1,
      container = false,
      WidgetType = "text",
      text = " [confirm] [0x46f1bb15]",
      font = "english_18",
      scale = 1,
      Justification = "left",
      EventHandlerFile = "",
      EventHandlers = {},
      EventHandlerNames = {},
      Children = {}
    },
    {
      name = "Movie subtitle",
      x1 = 70,
      y1 = 416,
      x2 = 570,
      y2 = 427,
      RedLevel = 255,
      GreenLevel = 255,
      BlueLevel = 255,
      TranslucencyLevel = 255,
      HorizontalAnchor = "center",
      VerticalAnchor = "bottom",
      visible = 1,
      container = false,
      WidgetType = "text",
      text = " ",
      font = "english_18",
      scale = 1,
      Justification = "center",
      EventHandlerFile = "MrxGuiCinematic",
      EventHandlers = {
        GuiInitialization = MrxGuiCinematic._InitializeSubtitleBuffer
      },
      EventHandlerNames = {
        GuiInitialization = "_InitializeSubtitleBuffer"
      },
      Children = {}
    },
    {
      name = "Movie supersubtitle",
      x1 = 70,
      y1 = 48,
      x2 = 570,
      y2 = 70,
      RedLevel = 255,
      GreenLevel = 255,
      BlueLevel = 255,
      TranslucencyLevel = 255,
      HorizontalAnchor = "center",
      VerticalAnchor = "top",
      visible = 1,
      container = false,
      WidgetType = "text",
      text = " ",
      font = "english_18",
      scale = 1,
      Justification = "center",
      EventHandlerFile = nil,
      EventHandlers = {},
      EventHandlerNames = {},
      Children = {}
    }
  }
}
if import then
  import("MrxGuiBase")
end

function ReInit()
  for i in pairs(AddedWidgetList) do
    MrxGuiBase.RemoveWidget(AddedWidgetList[i])
  end
  AddedWidgetList = {}
  for i in pairs(LocalWidgetList) do
    MrxGuiBase.LoadAndAddWidgetFromLayoutFileData(LocalWidgetList[i], AddedWidgetList)
  end
end
