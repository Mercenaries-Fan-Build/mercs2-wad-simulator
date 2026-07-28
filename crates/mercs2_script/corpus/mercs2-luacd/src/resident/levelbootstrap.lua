function LoadLevel(LevelName, MasterScript, Reload)
  if LevelName == nil then
    LevelName = Sys.GetLevelName()
  end
  if MasterScript == nil then
    MasterScript = Sys.GetMasterScriptName()
  end
  Sys.SetLevelName(LevelName)
  Sys.SetMasterScriptName(MasterScript)
  Debug.Printf("Loading " .. LevelName .. " level with " .. MasterScript .. " masterscript")
  if Reload == nil then
    Reload = false
  end
  Sys.RequiredAsset(LevelName .. "_base", "layer", -2, false)
  Sys.RequiredAsset(MasterScript, "script", -3, false)
  Sys.RequestGameState("Loading")
end
