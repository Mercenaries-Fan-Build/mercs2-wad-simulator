function Init()
  tFortressNodes = {}
  
  tAdjacencyTable = {}
  tAdjacencyTable["0x024BE2A6"] = {"Slice2B"}
  tAdjacencyTable["0x84536B11"] = {"Slice2A", "Slice2C"}
  tAdjacencyTable["0x8A5135EC"] = {"Slice2B", "Slice2D"}
  tAdjacencyTable["0x0C58BE57"] = {"Slice2C", "Slice2E"}
  tAdjacencyTable["0xAA55E57A"] = {"Slice2D", "Slice2F"}
  tAdjacencyTable["0x0C5D3B85"] = {"Slice2E", "Slice2G"}
  tAdjacencyTable["0x225B1F90"] = {"Slice2F", "Slice2H"}
  tAdjacencyTable["0x043A9CAB"] = {"Slice2G", "Slice2I"}
  tAdjacencyTable["0x22388D4E"] = {"Slice2H", "Slice2J"}
  tAdjacencyTable["0x843FE359"] = {"Slice2I", "Slice2K"}
  tAdjacencyTable["0x2A3D1714"] = {"Slice2J", "Slice2L"}
  tAdjacencyTable["0x8C446D1F"] = {
    "Slice2K",
    "Slice2M",
    "Slice4A"
  }
  tAdjacencyTable["0xAA425DC2"] = {
    "Slice2L",
    "Slice2N",
    "Slice4A"
  }
  tAdjacencyTable["0x2C49E62D"] = {"Slice2M", "Slice2O"}
  tAdjacencyTable["0x02476578"] = {"Slice2N"}
end

function Deinit()
  tFortressNodes = nil
  tAdjacencyTable = nil
end

function OnDeactivate(uGuid, args)
  local tNodeList = tFortressNodes[uGuid]
  if tNodeList then
    Debug.Printf(" ==**== Deactivating fortress ", uGuid, " ", #tNodeList)
    if Object.IsAlive(uGuid) then
      Object.Kill(uGuid)
    end
    for name, e in pairs(tNodeList) do
      if type(e) == "userdata" then
        Event.Delete(e)
      end
    end
    tFortressNodes[uGuid] = nil
  end
end

function OnStateChange(uGuid, uiNodeHashName, uiStateHashName)
  local sStateHashName = Sys.GuidToString(uiStateHashName)
  local sNodeHashName = Sys.GuidToString(uiNodeHashName)
  if sNodeHashName == "0xCF37044A" and sStateHashName == "0x694683EB" then
    tFortressNodes[uGuid] = {}
    local tStart = {"Slice2B", "Slice2C"}
    local i = Math.randi(#tStart)
    KillNode(uGuid, tStart[i])
  end
end

function KillNode(uGuid, sNodeName)
  local tNodeList = tFortressNodes[uGuid]
  Debug.Printf(" ==**== Killing node ", uGuid, " / ", sNodeName, " ", tNodeList[sNodeName])
  if tNodeList[sNodeName] then
    return
  end
  local tTime
  local uNodeNameHash = String.GetHash(sNodeName)
  if Object.GetNodeHealth(uGuid, sNodeName) > 0 then
    ObjectState.SendDamage(uGuid, uNodeNameHash, 1)
    tTime = Math.randf(0.3, 0.5)
  else
    tTime = Math.randf(0.7, 1)
  end
  local tAdjacentNodes = tAdjacencyTable[Sys.GuidToString(uNodeNameHash)]
  if tAdjacentNodes then
    tNodeList[sNodeName] = Event.Create(Event.TimerRelative, {tTime}, KillNodeSet, {uGuid, tAdjacentNodes})
  else
    tNodeList[sNodeName] = true
  end
end

function KillNodeSet(uGuid, tTable)
  for i, node in ipairs(tTable) do
    KillNode(uGuid, node)
  end
end
