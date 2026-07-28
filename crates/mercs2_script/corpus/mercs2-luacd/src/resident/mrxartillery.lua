inherit("MrxSupport")
import("MrxSupportDesignatorBeacon")
import("MrxVoSequence")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorBeacon:Create()
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Fiona")
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle or "Fiona"
  if self.sDeliveryVehicle then
    oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  else
    oNewSupport.uDeliveryVehicle = Pg.GetGuidByName("Fiona")
  end
  oNewSupport:SetModuleName("MrxArtillery")
  return oNewSupport
end

function DesignationCallback(self)
  local uHero = Player.GetCharacter(self.uOwner)
  self.sAmmo = "Artillery Shell"
  self.nWidth = 25
  local nShells = 12
  local nTime = 8
  local nTargetX, nTargetY, nTargetZ, uBeacon = self:GetDesignator():GetTarget()
  for i = 1, nShells do
    Event.Create(Event.TimerRelative, {
      3 + i * (nTime / nShells)
    }, TriggerFallingMissile, {self})
    if uBeacon and Player.IsLocal(self.uOwner) then
      Event.Create(Event.TimerRelative, {
        3.5 + nTime
      }, Object.Remove, {uBeacon})
    end
  end
  local tVO = {}
  local Owner = self.uDeliveryVehicle
  if Object.HasLabel(Owner, "Allied") then
    tVO = {
      "AlliedSoldier01.Support.Incoming01",
      "AlliedSoldier01.Support.Incoming02",
      "AlliedSoldier01.Support.Incoming03"
    }
  elseif Object.HasLabel(Owner, "China") then
    tVO = MrxUtil.GetRandomTableElement({
      "ChinaSoldier01.Support.Artillery01",
      "ChinaSoldier01.Support.Artillery02",
      "ChinaSoldier01.Support.Incoming01",
      "ChinaSoldier01.Support.Incoming02",
      "ChinaSoldier01.Support.Incoming03"
    })
  elseif Object.HasLabel(Owner, "Guerilla") then
    tVO = MrxUtil.GetRandomTableElement({
      "GurSoldier01.Support.Artillery01",
      "GurSoldier01.Support.Artillery02"
    })
  else
    tVO = {
      "Fiona-In-Mission-Contract-Jet01-03"
    }
  end
  MrxVoSequence.Start(tVO, nil, MrxVoSequence.knPriorityFreeplay, false)
end

function TriggerFallingMissile(self)
  local nTargetX, nTargetY, nTargetZ, uBeacon = self:GetDesignator():GetTarget()
  if uBeacon then
    nTargetX, nTargetY, nTargetZ = Object.GetPosition(uBeacon)
  end
  if not nTargetX then
    return
  end
  nTargetX = nTargetX + math.randf() * self.nWidth - math.randf() * self.nWidth
  nTargetZ = nTargetZ + math.randf() * self.nWidth - math.randf() * self.nWidth
  local uOrdnanceGuid = Airstrike.SpawnOrdnance(self.sAmmo, nTargetX, nTargetY + 200, nTargetZ, 0, -100, 0, "impact", 1, self.uOwner)
end
