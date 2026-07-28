inherit("Blippable")
import("MrxGui")
import("MrxPmc")
import("MrxSupportData")
import("MrxTutorialManager")
import("MrxUtil")
import("MrxVoSequence")
_kDistance = 150
local tMunitions = {
  "artillery",
  "bombingrun",
  "bunkerbuster",
  "carpetbomb",
  "clusterbomb",
  "combatairpatrol",
  "cruisemissile",
  "daisycutter",
  "fuelairbomb",
  "harm",
  "laserguidedbomb",
  "moab",
  "rocketartillery",
  "smartbomb",
  "strategicmissile",
  "surgicalstrike",
  "tankbuster",
  {nFuel = 50},
  {nFuel = 500},
  {nFuel = 5000},
  {nCash = 100000}
}
_nTagged = 0

function OnActivate(uGuid, uRuntimeOwner, nStock)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Awake, {uGuid, nStock})
end

_tStatusList = {}

function Awake(uGuid, nStock)
  local oPrototype = getfenv()
  if Object.GetHealth(uGuid) == 0 then
    return
  end
  if not tMunitions[nStock] then
  end
  oPrototype.sTexture = "radar_Munition"
  oPrototype.tColor = {
    51,
    102,
    51
  }
  oPrototype.tFlash = {
    255,
    255,
    255
  }
  oPrototype.nSize = 8
  oPrototype.tMarker = {
    sTexture = "pickup_munitions",
    tColor = {
      153,
      255,
      153
    },
    nSize = 48
  }
  local oInstance = oPrototype:Create(uGuid, nStock)
  oInstance:SetBlipped()
  oInstance.nStock = nStock
  Event.Create(Event.WeaponEvent, {
    "hero",
    "pickup",
    "Laptop"
  }, PickupMunitions, {oInstance})
end

function Delete(oSelf)
  local uGuid = oSelf.uGuid
  oSelf:ClearBlipped()
  if oSelf.TaggedMarker then
    Marker.Remove(oSelf.TaggedMarker)
    if Net.IsServer() then
      Net.SendEvent_RemoveMarkerObjective(oSelf.TaggedMarker)
    end
    oSelf.TaggedMarker = nil
    _nTagged = _nTagged - 1
  end
  if oSelf._uHideMessage then
    Event.Delete(oSelf._uHideMessage)
    oSelf._uHideMessage = nil
  end
  Blippable.Delete(oSelf)
end

function OnDeath(uGuid)
  if not _tStatusList then
    _tStatusList = {}
  end
  Inheritable.OnDeath(uGuid)
end

function PickupMunitions(oInstance)
  local oPda = MrxGui.GetWidgetByNameAndOwner("PDA", Player.GetLocalPlayer())
  if not oPda then
    Debug.Printf("ERROR: No PDA found!")
    return
  end
  local uGuid = oInstance.uGuid
  local nStock = oInstance.nStock
  local vStock = tMunitions[nStock]
  Event.Post("MunitionsPickup", {vStock, uGuid})
  MrxPmc.AddSupportQty(vStock, 1, true)
  oPda:UpdateSupport(vStock, nil, nil, MrxPmc.GetSupportQty(vStock))
  local tCues = {
    "Fiona.Support.Munitions02",
    "Fiona.Support.Munitions03"
  }
  local sCue = MrxUtil.GetRandomTableElement(tCues)
  MrxVoSequence.Start(sCue, nil, MrxVoSequence.knPriorityFreeplay)
end
