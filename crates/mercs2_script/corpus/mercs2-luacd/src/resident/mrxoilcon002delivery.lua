inherit("MrxSupportDelivery")
import("MrxSupportDesignator")
import("MrxSubtitle")
import("MrxOilCon002Delivery")
local _tDeliveryLocations = {}
NETEVENT_SETDELIVERYLOCATIONS = 0

function NetEventCallback(nEventType, tArgs)
  Debug.Printf("NetEventCallback")
  if nEventType == NETEVENT_SETDELIVERYLOCATIONS then
    ResetDropZones()
    local i = table.getn(_tDeliveryLocations)
    while 1 <= i do
      if tArgs[i] == 0 then
        Debug.Printf("removing drop zone " .. tostring(i))
        RemoveDropZone(i)
      end
      i = i - 1
    end
  end
end

function NetSendDropZones(bPlayerJoined)
  if not Net.IsServer() then
    return
  end
  Debug.Printf("NetSendDropZones")
  local nLoc1 = 0
  local nLoc2 = 0
  local nLoc3 = 0
  for _, sLocation in ipairs(_tDeliveryLocations) do
    if sLocation == "oilcon002_loc_postA" then
      nLoc1 = 1
    elseif sLocation == "oilcon002_loc_postB" then
      nLoc2 = 1
    elseif sLocation == "oilcon002_loc_postC" then
      nLoc3 = 1
    end
  end
  Debug.Printf("nLoc1 = " .. tostring(nLoc1))
  Debug.Printf("nLoc2 = " .. tostring(nLoc2))
  Debug.Printf("nLoc3 = " .. tostring(nLoc3))
  Net.SendCustomEvent("MrxOilCon002Delivery", NETEVENT_SETDELIVERYLOCATIONS, {
    nLoc1,
    nLoc2,
    nLoc3
  }, true)
end

function ResetDropZones()
  _tDeliveryLocations = {
    "oilcon002_loc_postA",
    "oilcon002_loc_postB",
    "oilcon002_loc_postC"
  }
  NetSendDropZones()
end

function AddSupport()
  for _, uPlayer in ipairs(Player.GetAllPlayers()) do
    local oSupportMenu = MrxGui.GetWidgetByNameAndOwner("Support Menu", uPlayer)
    if oSupportMenu then
      oSupportMenu:AddItem({
        sName = "[support.supply.listeningpost.name]",
        sIcon = "vehicles_helir_uh1",
        oSupport = MrxOilCon002Delivery:Create(uPlayer)
      })
    end
  end
end

function RemoveSupport()
  for _, uPlayer in ipairs(Player.GetAllPlayers()) do
    local oSupportMenu = MrxGui.GetWidgetByNameAndOwner("Support Menu", uPlayer)
    if oSupportMenu then
      oSupportMenu:RemoveItem("Listening Post Delivery")
    end
  end
end

function Create(oSelf, uOwnerGuid)
  local oNewSupport = MrxSupportDelivery:Create(uOwnerGuid)
  oNewSupport.Create = Create
  oNewSupport:SetCargo("Listening Post")
  local oDesignator = oNewSupport:GetDesignator()
  oDesignator:SetAATestLevel("none")
  oDesignator:SetValidationFunction(_ValidateDropZone)
  oNewSupport:SetModuleName("MrxOilCon002Delivery")
  if not _ePlayerJoin then
    _ePlayerJoin = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, NetSendDropZones, {true})
  end
  return oNewSupport
end

function _ValidateDropZone(fCallback, nX, nY, nZ, oSupport)
  local bInsideLocation = false
  for _, sLocation in pairs(GetCurrentDropZones()) do
    local uGuid = Pg.GetGuidByName(sLocation)
    if uGuid == nil then
      bInsideLocation = true
      break
    end
    local nDistance = GetDistanceToObject(uGuid, nX, nY, nZ)
    Debug.Printf("Grenade is " .. tostring(nDistance) .. "m from location " .. tostring(Object.GetName(uGuid)))
    if nDistance <= 30 then
      bInsideLocation = true
      nX, nY, nZ = Object.GetPosition(uGuid)
      break
    end
  end
  if bInsideLocation then
    fCallback(true, nX, nY, nZ, oSupport)
    MrxSupportDesignator.ValidateGroundDropZone(fCallback, nX, nY, nZ, oSupport)
  else
    fCallback(false, "oilcon002_toofar")
  end
end

function GetDistanceToObject(uObjectA, nX, nY, nZ, bIgnoreY)
  local x1, y1, z1 = Object.GetPosition(uObjectA)
  local dx = x1 - nX
  local dy = y1 - nY
  local dz = z1 - nZ
  if bIgnoreY then
    dy = 0
  end
  return Math.Length(dx, dy, dz)
end

function GetDistanceBetween(uObjectA, uObjectB, bIgnoreY)
  local x1, y1, z1 = Object.GetPosition(uObjectA)
  local x2, y2, z2 = Object.GetPosition(uObjectB)
  local dx = x1 - x2
  local dy = y1 - y2
  local dz = z1 - z2
  if bIgnoreY then
    dy = 0
  end
  return Math.Length(dx, dy, dz)
end

function RemoveDropZone(nIndex)
  table.remove(_tDeliveryLocations, nIndex)
  NetSendDropZones()
end

function GetCurrentDropZones()
  return _tDeliveryLocations
end
