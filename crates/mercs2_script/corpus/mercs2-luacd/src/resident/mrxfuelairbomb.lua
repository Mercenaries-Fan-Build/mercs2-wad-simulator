inherit("MrxSupport")
import("MrxSupportDesignatorSmoke")
import("MrxSupportDesignatorLaser")
import("MrxSupportDesignatorSatellite")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorSmoke:Create()
  oDesignator:SetValidationFunction(nil)
  oDesignator:SetAATestLevel("basic")
  oDesignator:SetTargetValidationRequired(false)
  oDesignator:SetSmokeColor("red")
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Pilot")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetModuleName("MrxFuelAirBomb")
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  return oNewSupport
end

function DesignationCallback(self)
  local nTargetXOffset = 0
  local nTargetZOffset = 20
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(300, 60, -1, self.uOwner)
  local nLaunchX, nLaunchY, nLaunchZ = Pg.FindPointFromCamera(65, 60)
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
  local uJet = Airstrike.Flyby(self.uDeliveryVehicle, nSpawnX, nSpawnZ, nLaunchX, nLaunchZ, nLaunchY, 200, DropBomb, {self})
  self.uJet = uJet
  MrxSupport.PlayAirstrikeVO(uJet, {
    "Misha-None-Freeplay-Support-06",
    "Misha-None-Freeplay-Support-07",
    "Misha-None-Freeplay-Support-16",
    "Misha-None-Freeplay-Support-24"
  })
end

function DropBomb(self)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uJet)
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = self:GetDesignator():GetTarget()
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  local nDistance = Math.Length(nVectorX, nVectorY, nVectorZ) - 24
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  local nSpeedScale = 60
  self.uSpawnedBomb = Airstrike.SpawnOrdnance("Fuel Air Bomb Projectile", nSpawnX, nSpawnY - 5, nSpawnZ, nVectorX * nSpeedScale, nVectorY * nSpeedScale, nVectorZ * nSpeedScale, "distance", nDistance, self.uOwner, BombExplodes, {self, "distance"})
end

function BombExplodes(self, sTrigger)
  local nBombX, nBombY, nBombZ = Object.GetPosition(self.uSpawnedBomb)
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = self:GetDesignator():GetTarget()
  local nVectorX, nVectorY, nVectorZ = nTargetX - nBombX, nTargetY - nBombY, nTargetZ - nBombZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  local uExplosion = Airstrike.SpawnDirectedObject("global_particle_airstrike_fuelairbomb", nBombX, nBombY, nBombZ, -nVectorX, -nVectorY, -nVectorZ)
  local uExplosionA = Airstrike.SpawnDirectedObject("global_particle_explosion_flash_large", nBombX, nBombY, nBombZ, -nVectorX, -nVectorY, -nVectorZ)
  Event.Create(Event.TimerRelative, {1.6}, Ignition, {
    nBombX,
    nBombY,
    nBombZ,
    nTargetX,
    nTargetY,
    nTargetZ,
    nVectorX,
    nVectorY,
    nVectorZ
  })
end

function Test(nVZ, nVY, nVZ, nTargetX, nTargetY, nTargetZ)
  Airstrike.SpawnDirectedObject("global_particle_airstrike_fuelairbomb", nTargetX, nTargetY, nTargetZ, nVZ, nVY, nVZ)
end

function Ignition(nBombX, nBombY, nBombZ, nTargetX, nTargetY, nTargetZ, nVectorX, nVectorY, nVectorZ)
  Pg.Spawn("Light_airstrike_fuelairbomb_sml", nTargetX, nTargetY + 1, nTargetZ)
  Pg.Spawn("global_particle_exp_falling_debris_airstrike", nTargetX, nTargetY + 1, nTargetZ)
  Event.Create(Event.TimerRelative, {0.15}, Fireball, {
    nBombX,
    nBombY,
    nBombZ,
    nTargetX,
    nTargetY,
    nTargetZ,
    nVectorX,
    nVectorY,
    nVectorZ
  })
  Sound.CueSound(0, "exp_oiltrucker")
end

function Fireball(nBombX, nBombY, nBombZ, nTargetX, nTargetY, nTargetZ, nVectorX, nVectorY, nVectorZ)
  Pg.Spawn("Explosion (Fuel Air Bomb)", nBombX, nBombY - 2, nBombZ)
  Pg.Spawn("Light_airstrike_fuelairbomb_lrg_flash", nBombX, nBombY - 2, nBombZ)
  Pg.Spawn("global_particle_exp_shockwave_ground", nBombX, nTargetY, nBombZ)
end
