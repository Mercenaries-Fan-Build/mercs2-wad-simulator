inherit("MrxSupport")
import("MrxSupportDesignatorBeacon")
local sProjectileName = "Cruise Missile Projectile"
local sExplosionName = "Explosion (Cruise Missile)"
local sBomb = "Support Vehicle (Cruise Missile)"
local uBomb = Pg.GetGuidByName("Support Vehicle (Cruise Missile)")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorBeacon:Create()
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Fiona")
  oNewSupport:SetModuleName("MrxCruiseMissile")
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle or "Fiona"
  if self.sDeliveryVehicle then
    oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  else
    oNewSupport.uDeliveryVehicle = Pg.GetGuidByName("Fiona")
  end
  oNewSupport.sBomb = self.sBomb
  oNewSupport.uBomb = Pg.GetGuidByName(self.sBomb)
  return oNewSupport
end

function DesignationCallback(self)
  Debug.Printf(tostring(self))
  local nTargetXOffset = 20
  local nTargetZOffset = 100
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(300, 25, -1, self.uOwner, 720 + math.randi(180))
  local nTargetX, nTargetY, nTargetZ = self:GetDesignator():GetTarget()
  local nOffsetX = (nTargetZ - nSpawnZ) * -1
  local nOffsetZ = nTargetX - nSpawnX
  local nNearOffsetX, nNearOffsetY, nNearOffsetZ = Math.Normalize(nOffsetX, 0, nOffsetZ)
  nNearOffsetX = nNearOffsetX * nTargetXOffset
  nNearOffsetZ = nNearOffsetZ * nTargetXOffset
  local nNearerX = nSpawnX - nTargetX
  local nNearerY = 0
  local nNearerZ = nSpawnZ - nTargetZ
  nNearerX, nNearerY, nNearerZ = Math.Normalize(nNearerX, 0, nNearerZ)
  nNearerX = nNearerX * nTargetZOffset
  nNearerZ = nNearerZ * nTargetZOffset
  self.uJet = Airstrike.Flyby("Support Vehicle (Cruise Missile)", nSpawnX, nSpawnZ, nSpawnX + nNearOffsetX + (nTargetX - nSpawnX) + nNearerX, nSpawnZ + nNearOffsetZ + (nTargetZ - nSpawnZ) + nNearerZ, nTargetY + 150, 50, DropBomb, {self})
  local tVO = {}
  local Owner = self.uDeliveryVehicle
  if Object.HasLabel(Owner, "Allied") then
    tVO = {
      "AlliedSoldier01.Support.Incoming01",
      "AlliedSoldier01.Support.Incoming03"
    }
  elseif Object.HasLabel(Owner, "China") then
    tVO = MrxUtil.GetRandomTableElement({
      "ChinaSoldier01.Support.Artillery01",
      "ChinaSoldier01.Support.Incoming01",
      "ChinaSoldier01.Support.Incoming02",
      "ChinaSoldier01.Support.Incoming03"
    })
  else
    tVO = {
      "Fiona-In-Mission-Contract-Jet01-03"
    }
  end
  MrxVoSequence.Start(tVO, nil, MrxVoSequence.knPriorityFreeplay, false)
end

function DropBomb(self)
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = self:GetDesignator():GetTarget()
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uJet)
  Object.Remove(self.uJet)
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  self.uSpawnedBomb = Airstrike.SpawnTargettedOrdnance(sProjectileName, nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, uGuid, "impact", 0, self.uOwner, BombExplodes, {self})
end

function BombExplodes(self)
  Debug.Printf("BOOOOOOOOOOOOOOOOOOOOOOM")
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = self:GetDesignator():GetTarget()
  if uGuid and Player.IsLocal(self.uOwner) then
    Object.Remove(uGuid)
  end
end
