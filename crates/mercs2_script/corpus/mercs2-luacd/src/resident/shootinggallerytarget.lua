function OnStateChange(uGuid, uiNodeHashName, uiStateHashName)
  local sStateHashName = Sys.GuidToString(uiStateHashName)
  
  if sStateHashName == "0x7687DF41" then
    Vehicle.OpenDoor(uGuid, "pivot")
  elseif sStateHashName == "0xACB51200" then
    Vehicle.CloseDoor(uGuid, "pivot")
  end
end
