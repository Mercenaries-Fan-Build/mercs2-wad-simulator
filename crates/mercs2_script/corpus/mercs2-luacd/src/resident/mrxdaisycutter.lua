inherit("MrxSupport")
import("MrxSupportDesignatorSmoke")
import("MrxUtil")
local sProjectileName = "Daisy Cutter Projectile"
local sExplosionName = "Explosion (Daisy Cutter)"
local sDeliveryVehicle = "Support Vehicle (C130)"

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorSmoke:Create()
  oDesignator:SetValidationFunction(_NoValidation)
  oDesignator:SetAATestLevel("basic")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Fiona")
  oNewSupport:SetModuleName("MrxDaisyCutter")
  oNewSupport.sDeliveryVehicle = sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(sDeliveryVehicle)
  oNewSupport.sBomb = sProjectileName
  oNewSupport.uBomb = Pg.GetGuidByName(sProjectileName)
  return oNewSupport
end

function DesignationCallback(self)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(-300, 100, -1, self.uOwner)
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(0, 100, -1, self.uOwner)
  local uJet = Airstrike.Flyby(self.uDeliveryVehicle, nSpawnX, nSpawnZ, nTargetX, nTargetZ, nTargetY, 80, DropBomb, {self})
  self.uJet = uJet
  MrxSupport.PlayAirstrikeVO(uJet, {""})
end

function DropBomb(self)
  Debug.Printf("DropBomb")
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uJet)
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = self:GetDesignator():GetTarget()
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  local nSpeedScale = 20
  self.uSpawnedBomb = Airstrike.SpawnOrdnance(self.uBomb, nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, uTarget, "impact", self.uOwner, BombExplodes, {self})
end

function BombExplodes(self)
  Debug.Printf("BombExplodes")
  Event.Create(Event.TimerRelative, {1.5}, CreateDebris, {self})
end

function CreateDebris(self)
  Debug.Printf("CreateDebris")
  local x, y, z = self:GetDesignator():GetTarget()
  if Net.IsClient() or MrxUtil.GetDistanceToObject(Player.GetLocalCharacter(), x, y, z) > 150 then
    return
  end
  x, y, z = Object.GetPosition(Player.GetLocalCharacter())
  Graphics.Effect.Terrain("global_particle_dustfall", 20, 0.005, x, 8, z)
end
